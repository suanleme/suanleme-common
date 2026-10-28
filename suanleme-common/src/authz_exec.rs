//! 共享鉴权执行辅助（REQ-08 D290「网关统一编排、业务最小接入」的公共面）。
//!
//! 这里是**唯一**维护以下三件事的地方，业务服务不再各写一份：
//!
//! 1. **查询条件 → 本库 SQL 行谓词**的通用编译器 [`compile_query_filter`]：
//!    `QueryConstraint`/`TransitionQueryConstraint` 语义（selected_scope、allow/deny 配对、
//!    资源模式只接受可证明等价 `any`、未确认 NULL 的完整性闸、SQL 三值逻辑）集中在本模块；
//!    业务只提供**真实列名**（[`ProjectScopeColumns`]）与表别名前缀，并负责把 `params`
//!    绑定到本库 SQL。
//! 2. **保留原截止的 permit** [`QueryPermit`]：从约束携带的真实源截止构造，提供
//!    [`QueryPermit::ensure_live`] 半开区间复验；命中/等待/重试**都不续期**。
//!    业务必须在分页/count/summary 之前应用谓词，并在实际数据接受点再次 `ensure_live`。
//! 3. **信封/回显最小校验**：[`check_selected_scope_echo`]、[`check_action`]、[`check_subject`]、
//!    [`SourceDeadline`]。网关必须核对 IAM 响应的 `selected_scope` 与请求 typed 相等
//!    （异常回显不能把单项目请求扩大为 all）；业务 guard 用同一份主体/动作/截止语义。
//!
//! 本模块**不读策略、不调用 IAM、不计算用户授权**；它只执行已由网关/IAM 产出的约束。
//! 不支持的资源模式显式失败（[`ExecError::ResourceMappingUnverified`]），不截断/不降级为 all。

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::authz::{Clause, ClauseScope, Id, RequestAction, RequestSubject, ResourcePattern, SelectedScope};
use crate::authz_transition::TransitionQueryConstraint;

/// 共享执行辅助的失败原因（结构化、具名；不含 SQL/存在性细节）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecError {
    /// 缺少可信执行上下文（enforce 模式下客户端直连/网关未注入）。
    ContextMissing,
    /// 线格式/协议版本不支持（schema/V2 混用等）。
    ProtocolUnsupported,
    /// audience 与当前业务服务不一致。
    AudienceMismatch,
    /// action 与 handler 登记路由不一致。
    ActionMismatch,
    /// 主体与请求信封不一致。
    SubjectMismatch,
    /// 业务输入摘要重算不一致。
    RequestDigestMismatch,
    /// 源截止已到（半开区间 `now >= expires`）。
    DeadlineExpired,
    /// 查询约束缺失/域不符/与信封不一致。
    QueryConstraintInvalid,
    /// 约束里的资源模式在本库没有可证明等价的映射（fail-closed，不声明 V2 等价）。
    ResourceMappingUnverified,
    /// 价格/审计/数据库等权威不可用（不得解释为 allow）。
    AuthorityUnavailable,
}

impl ExecError {
    /// 结构化原因 token（审计/日志/内部错误 message 用，不含 SQL 细节）。
    pub fn token(self) -> &'static str {
        match self {
            Self::ContextMissing => "context_missing",
            Self::ProtocolUnsupported => "protocol_unsupported",
            Self::AudienceMismatch => "audience_mismatch",
            Self::ActionMismatch => "action_mismatch",
            Self::SubjectMismatch => "subject_mismatch",
            Self::RequestDigestMismatch => "request_digest_mismatch",
            Self::DeadlineExpired => "deadline_expired",
            Self::QueryConstraintInvalid => "query_constraint_invalid",
            Self::ResourceMappingUnverified => "resource_mapping_unverified",
            Self::AuthorityUnavailable => "authority_unavailable",
        }
    }
}

impl fmt::Display for ExecError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.token())
    }
}

impl std::error::Error for ExecError {}

/// 从网关认证后信封得到的可信身份（tenant/user/tenant_type 来自认证，不读业务 data）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnvelopeIdentity {
    pub tenant_id: i64,
    pub user_id: Option<i64>,
    pub tenant_type: Option<String>,
}

impl EnvelopeIdentity {
    /// 构造可信身份：`tenant_id` 必须为正整数；user/tenant_type 可缺（已登记缺口）。
    pub fn from_parts(
        tenant_id: Option<i64>,
        user_id: Option<i64>,
        tenant_type: Option<&str>,
    ) -> Option<Self> {
        let tenant_id = tenant_id.filter(|value| *value > 0)?;
        Some(Self {
            tenant_id,
            user_id,
            tenant_type: tenant_type.map(str::to_owned),
        })
    }
}

/// 真实源截止（毫秒）。缓存层只能缩短，不得续期。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceDeadline {
    pub source_started_at_unix_ms: i64,
    pub source_expires_at_unix_ms: i64,
}

impl SourceDeadline {
    /// 从真实源起止构造；起止倒置/负数即拒绝（不修正、不放宽）。
    pub fn new(source_started_at_unix_ms: i64, source_expires_at_unix_ms: i64) -> Result<Self, ExecError> {
        if source_started_at_unix_ms < 0
            || source_expires_at_unix_ms <= source_started_at_unix_ms
        {
            return Err(ExecError::QueryConstraintInvalid);
        }
        Ok(Self {
            source_started_at_unix_ms,
            source_expires_at_unix_ms,
        })
    }

    /// 半开区间复验：`now >= expires` 即到期；墙钟回退不会延长截止。
    pub fn ensure_live(&self, now_unix_ms: i64) -> Result<(), ExecError> {
        if now_unix_ms >= self.source_expires_at_unix_ms {
            return Err(ExecError::DeadlineExpired);
        }
        Ok(())
    }

    /// 剩余毫秒（可为负；调用方应先用 [`Self::ensure_live`]）。
    pub fn remaining_ms(&self, now_unix_ms: i64) -> i64 {
        self.source_expires_at_unix_ms.saturating_sub(now_unix_ms)
    }
}

/// 业务本库的项目归属列配置（业务只配置真实列名，不配置策略）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProjectScopeColumns {
    /// 项目归属列（如 `resource_group_id`）。
    pub resource_group_id: &'static str,
    /// 「新建明确无项目 / 历史未分类」事实位列（如 `project_ownership_confirmed`）。
    /// `None` 表示本表没有该列（不得把未确认 NULL 当无项目）。
    pub ownership_confirmed: Option<&'static str>,
}

impl ProjectScopeColumns {
    pub const fn new(
        resource_group_id: &'static str,
        ownership_confirmed: Option<&'static str>,
    ) -> Self {
        Self {
            resource_group_id,
            ownership_confirmed,
        }
    }
}

/// 编译选项：资源域、占位符起始序号、表别名前缀、历史全量确认开关。
#[derive(Debug, Clone, Copy)]
pub struct QueryCompileOptions<'a> {
    pub expected_domain: &'a str,
    pub start_index: usize,
    /// 项目列所在表的别名前缀（单表为 `""`，JOIN 场景如 `"n."`/`"r."`）。
    pub prefix: &'a str,
    /// 仅在**已取得全部历史行归属事实**时由业务显式打开；默认 `false`（未确认 NULL 整行排除）。
    pub null_confirmed_ungrouped: bool,
}

/// 编译结果：可直接拼到 `WHERE tenant_id = $1 …` 之后的谓词与绑定参数。
///
/// 谓词只引用 `trusted_tenant` 之外的列；参数从 `start_index` 开始编号，调用方负责
/// 可信租户（`$1`）与后续分页参数的编号衔接。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompiledFilter {
    pub predicate: String,
    pub params: Vec<i64>,
}

/// 保留原截止的查询许可（`permit`）。
///
/// 与「只返回 SQL」的区别：携带真实源截止，业务在分页/count/summary 前用它绑谓词，
/// 并在实际读/写接受点调用 [`Self::ensure_live`] 复验；任何命中/等待/重试都**不续期**。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryPermit {
    pub filter: CompiledFilter,
    pub deadline: SourceDeadline,
    pub domain: String,
    pub selected_scope: SelectedScope,
    pub public_bypass: bool,
}

impl QueryPermit {
    /// 实际接受点复验：过期即拒绝（业务不得把过期 permit 当作已完成授权）。
    pub fn ensure_live(&self, now_unix_ms: i64) -> Result<(), ExecError> {
        self.deadline.ensure_live(now_unix_ms)
    }

    /// 剩余毫秒；调用方应把返回值当**上界**，不得据此续期。
    pub fn remaining_ms(&self, now_unix_ms: i64) -> i64 {
        self.deadline.remaining_ms(now_unix_ms)
    }
}

/// 编译 `TransitionQueryConstraint` 为行谓词。
///
/// 行谓词（data-model §2.1）：
/// ```text
/// trusted_tenant AND selected_scope(row)
///   AND (public_bypass OR ((OR applicable allow clauses(row)) AND NOT (OR applicable deny clauses(row))))
/// ```
/// 完整性闸（未确认 NULL ⇒ 归属未知 ⇒ 整行不返回）先于 allow/deny 求值。
pub fn compile_query_filter(
    constraint: &TransitionQueryConstraint,
    columns: ProjectScopeColumns,
    options: &QueryCompileOptions<'_>,
) -> Result<CompiledFilter, ExecError> {
    if constraint.domain != options.expected_domain {
        return Err(ExecError::QueryConstraintInvalid);
    }
    constraint
        .validate()
        .map_err(|_| ExecError::QueryConstraintInvalid)?;
    let mut compiler = Compiler {
        columns,
        prefix: options.prefix,
        next_index: options.start_index,
        params: Vec::new(),
    };
    let selected = compiler.selected_scope(&constraint.selected_scope)?;
    // 完整性闸：没有事实位的表不得把 NULL 当无项目；有事实位但未确认的 NULL 是归属未知。
    let completeness = match (options.null_confirmed_ungrouped, columns.ownership_confirmed) {
        (true, _) => format!(
            "({prefix}{group} > 0 OR {prefix}{group} IS NULL)",
            prefix = compiler.prefix,
            group = columns.resource_group_id
        ),
        (false, Some(confirmed)) => format!(
            "({prefix}{group} > 0 OR ({prefix}{group} IS NULL AND {prefix}{confirmed}))",
            prefix = compiler.prefix,
            group = columns.resource_group_id,
            confirmed = confirmed
        ),
        (false, None) => format!(
            "({prefix}{group} > 0)",
            prefix = compiler.prefix,
            group = columns.resource_group_id
        ),
    };
    let allow = compiler.clauses(&constraint.allow_clauses, "FALSE")?;
    let deny = compiler.clauses(&constraint.deny_clauses, "FALSE")?;
    let public = if constraint.public_bypass {
        "TRUE"
    } else {
        "FALSE"
    };
    let predicate = format!(
        "{completeness} AND ({selected}) AND ({public} OR (COALESCE(({allow}), FALSE) AND NOT COALESCE(({deny}), FALSE)))"
    );
    Ok(CompiledFilter {
        predicate,
        params: compiler.params,
    })
}

/// 编译并构造保留原截止的 [`QueryPermit`]。
pub fn compile_query_permit(
    constraint: &TransitionQueryConstraint,
    columns: ProjectScopeColumns,
    options: &QueryCompileOptions<'_>,
) -> Result<QueryPermit, ExecError> {
    let filter = compile_query_filter(constraint, columns, options)?;
    let deadline = SourceDeadline::new(
        constraint.auth_state.source_started_at_unix_ms,
        constraint.auth_state.source_expires_at_unix_ms,
    )?;
    Ok(QueryPermit {
        filter,
        deadline,
        domain: constraint.domain.clone(),
        selected_scope: constraint.selected_scope.clone(),
        public_bypass: constraint.public_bypass,
    })
}

struct Compiler<'a> {
    columns: ProjectScopeColumns,
    prefix: &'a str,
    next_index: usize,
    params: Vec<i64>,
}

impl Compiler<'_> {
    fn bind(&mut self, value: i64) -> String {
        let placeholder = format!("${}", self.next_index);
        self.next_index += 1;
        self.params.push(value);
        placeholder
    }

    fn group_column(&self) -> String {
        format!("{}{}", self.prefix, self.columns.resource_group_id)
    }

    /// `selected_scope`：只收窄；不得把 visible 并集/空头当授权。
    fn selected_scope(&mut self, scope: &SelectedScope) -> Result<String, ExecError> {
        match scope {
            SelectedScope::AllAuthorized => Ok("TRUE".to_owned()),
            SelectedScope::Project { project_id } => {
                let placeholder = self.bind(project_id.get());
                Ok(format!("({} = {placeholder})", self.group_column()))
            }
            SelectedScope::Ungrouped => Ok(format!("({} IS NULL)", self.group_column())),
        }
    }

    /// 子句 OR；每条子句 = scope_match AND (OR resource_patterns_match)。
    fn clauses(&mut self, clauses: &[Clause], empty: &'static str) -> Result<String, ExecError> {
        let mut parts = Vec::with_capacity(clauses.len());
        for clause in clauses {
            let scope = self.clause_scope(&clause.scope)?;
            let patterns = self.resource_patterns(&clause.resource_patterns)?;
            parts.push(format!("(({scope}) AND ({patterns}))"));
        }
        if parts.is_empty() {
            Ok(empty.to_owned())
        } else {
            Ok(parts.join(" OR "))
        }
    }

    /// `clause.scope` 与行项目的配对：
    /// * 已确认未分组行：D249 语义下全局与项目 grant 均可参与（项目 grant 的 scope_match
    ///   对无项目行也为 TRUE，与既有实现一致）；全局 grant 命中任意行；
    /// * 项目行：`all` 命中任意项目；`project(p)` 只命中 p。
    fn clause_scope(&mut self, scope: &ClauseScope) -> Result<String, ExecError> {
        match scope {
            ClauseScope::All => Ok("TRUE".to_owned()),
            ClauseScope::Project { project_id } => {
                let placeholder = self.bind(project_id.get());
                // 完整性闸已排除未分类 NULL；已确认无项目行沿 D249 接受项目 grant。
                let group = self.group_column();
                Ok(format!("(({group} IS NULL) OR ({group} = {placeholder}))"))
            }
        }
    }

    /// 资源模式只接受可证明等价的 `any`；其它模式 fail-closed。
    fn resource_patterns(&self, patterns: &[ResourcePattern]) -> Result<&'static str, ExecError> {
        if patterns
            .iter()
            .all(|pattern| matches!(pattern, ResourcePattern::Any))
        {
            // 语句空资源规范为 any（data-model §2.1）。
            Ok("TRUE")
        } else {
            Err(ExecError::ResourceMappingUnverified)
        }
    }
}

/// 回显核对：IAM 响应的 `selected_scope` 必须与本次请求 typed 相等。
///
/// 失败即拒绝：异常回显不能把「单项目」请求扩大成 `all_authorized`（D289/API §2.4）。
pub fn check_selected_scope_echo(
    requested: &SelectedScope,
    echoed: &SelectedScope,
) -> Result<(), ExecError> {
    if requested == echoed {
        Ok(())
    } else {
        Err(ExecError::QueryConstraintInvalid)
    }
}

/// 动作核对：`(method, path, route_contract_version)` 逐项相等（大小写按线格式规范）。
pub fn check_action(expected: &RequestAction, actual: &RequestAction) -> Result<(), ExecError> {
    if expected == actual {
        Ok(())
    } else {
        Err(ExecError::ActionMismatch)
    }
}

/// 主体核对：tenant 必须相等；信封给出 user 时必须相等；tenant_type 给出时必须相等。
pub fn check_subject(
    identity: &EnvelopeIdentity,
    actual: &RequestSubject,
) -> Result<(), ExecError> {
    if identity.tenant_id != actual.tenant_id.get() {
        return Err(ExecError::SubjectMismatch);
    }
    if let Some(user_id) = identity.user_id {
        if user_id != actual.user_id.get() {
            return Err(ExecError::SubjectMismatch);
        }
    }
    if let Some(tenant_type) = &identity.tenant_type {
        if tenant_type != &actual.tenant_type {
            return Err(ExecError::SubjectMismatch);
        }
    }
    Ok(())
}

/// 便捷构造项目 `SelectedScope`（测试/网关回显构造用）。
pub fn project_scope(project_id: i64) -> Result<SelectedScope, ExecError> {
    Id::new(project_id)
        .map(|project_id| SelectedScope::Project { project_id })
        .map_err(|_| ExecError::QueryConstraintInvalid)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::authz_transition::TransitionAuthState;
    use serde_json::json;

    const DOMAIN: &str = "deployment-project-transition-1";
    const COLUMNS: ProjectScopeColumns =
        ProjectScopeColumns::new("resource_group_id", Some("project_ownership_confirmed"));

    fn project(project_id: i64) -> SelectedScope {
        project_scope(project_id).unwrap()
    }

    fn clause_any(scope: ClauseScope) -> Clause {
        Clause {
            scope,
            resource_patterns: vec![ResourcePattern::Any],
        }
    }

    fn constraint(
        selected: SelectedScope,
        allow: Vec<Clause>,
        deny: Vec<Clause>,
        public_bypass: bool,
    ) -> TransitionQueryConstraint {
        TransitionQueryConstraint {
            domain: DOMAIN.to_owned(),
            selected_scope: selected,
            public_bypass,
            allow_clauses: allow,
            deny_clauses: deny,
            auth_state: TransitionAuthState::new(12, 34, 1_800_000_000_000, 1_800_000_060_000)
                .unwrap(),
        }
    }

    fn options(start_index: usize, prefix: &str) -> QueryCompileOptions<'_> {
        QueryCompileOptions {
            expected_domain: DOMAIN,
            start_index,
            prefix,
            null_confirmed_ungrouped: false,
        }
    }

    #[test]
    fn project_scope_allow_and_deny_compile_to_paired_row_predicate() {
        let compiled = compile_query_filter(
            &constraint(
                project(16),
                vec![
                    clause_any(ClauseScope::All),
                    clause_any(ClauseScope::Project {
                        project_id: Id::new(17).unwrap(),
                    }),
                ],
                vec![clause_any(ClauseScope::All)],
                false,
            ),
            COLUMNS,
            &options(2, "a."),
        )
        .unwrap();
        assert_eq!(compiled.params, vec![16, 17]);
        assert!(compiled.predicate.contains("(a.resource_group_id > 0"));
        assert!(compiled.predicate.contains("(a.resource_group_id = $2)"));
        assert!(compiled.predicate.contains("(a.resource_group_id = $3)"));
        assert!(compiled.predicate.contains("NOT COALESCE("));
    }

    #[test]
    fn empty_allow_is_empty_set_and_empty_deny_is_false() {
        let compiled = compile_query_filter(
            &constraint(SelectedScope::AllAuthorized, vec![], vec![], false),
            COLUMNS,
            &options(2, ""),
        )
        .unwrap();
        assert!(compiled.params.is_empty());
        assert!(compiled
            .predicate
            .contains("(FALSE OR (COALESCE((FALSE), FALSE) AND NOT COALESCE((FALSE), FALSE)))"));
    }

    #[test]
    fn unconfirmed_null_rows_are_excluded_by_the_completeness_gate() {
        let compiled = compile_query_filter(
            &constraint(SelectedScope::AllAuthorized, vec![], vec![], true),
            COLUMNS,
            &options(2, ""),
        )
        .unwrap();
        assert!(compiled.predicate.starts_with(
            "(resource_group_id > 0 OR (resource_group_id IS NULL AND project_ownership_confirmed))"
        ));
        // 没有事实位的表一律只认有项目行（不得把 NULL 当无项目）。
        let no_confirmed = ProjectScopeColumns::new("resource_group_id", None);
        let compiled = compile_query_filter(
            &constraint(SelectedScope::AllAuthorized, vec![], vec![], true),
            no_confirmed,
            &options(2, ""),
        )
        .unwrap();
        assert!(compiled.predicate.starts_with("(resource_group_id > 0)"));
    }

    #[test]
    fn non_any_resource_pattern_is_fail_closed() {
        let clause = Clause {
            scope: ClauseScope::All,
            resource_patterns: vec![ResourcePattern::WholeSegmentStar {
                resource_arn: "gjsl:deployment:*:4:task/*".to_owned(),
            }],
        };
        let error = compile_query_filter(
            &constraint(SelectedScope::AllAuthorized, vec![clause], vec![], false),
            COLUMNS,
            &options(2, ""),
        )
        .unwrap_err();
        assert_eq!(error, ExecError::ResourceMappingUnverified);
        assert_eq!(error.token(), "resource_mapping_unverified");
    }

    #[test]
    fn domain_mismatch_is_rejected() {
        let error = compile_query_filter(
            &constraint(SelectedScope::AllAuthorized, vec![], vec![], false),
            COLUMNS,
            &QueryCompileOptions {
                expected_domain: "deployment-project-v2",
                start_index: 2,
                prefix: "",
                null_confirmed_ungrouped: false,
            },
        )
        .unwrap_err();
        assert_eq!(error, ExecError::QueryConstraintInvalid);
    }

    #[test]
    fn permit_keeps_the_original_source_deadline_and_never_extends_it() {
        let permit = compile_query_permit(
            &constraint(project(16), vec![clause_any(ClauseScope::All)], vec![], false),
            COLUMNS,
            &options(1, ""),
        )
        .unwrap();
        assert_eq!(permit.deadline.source_started_at_unix_ms, 1_800_000_000_000);
        assert_eq!(permit.deadline.source_expires_at_unix_ms, 1_800_000_060_000);
        // 有效期内可接受。
        assert!(permit.ensure_live(1_800_000_030_000).is_ok());
        // 半开区间边界：恰好到期即拒绝。
        assert_eq!(
            permit.ensure_live(1_800_000_060_000).unwrap_err(),
            ExecError::DeadlineExpired
        );
        // 墙钟回退不延长：回退到更早时间也不改变原始截止。
        assert!(permit.ensure_live(1_799_999_999_000).is_ok());
        assert_eq!(
            permit.deadline.source_expires_at_unix_ms,
            1_800_000_060_000
        );
        // 该 permit 是从约束原样复制的截止；再次编译同一约束不会把截止推后。
        let again = compile_query_permit(
            &constraint(project(16), vec![clause_any(ClauseScope::All)], vec![], false),
            COLUMNS,
            &options(1, ""),
        )
        .unwrap();
        assert_eq!(again.deadline, permit.deadline);
    }

    #[test]
    fn permit_rejects_expired_source_window_at_construction() {
        // 起止倒置/负数 state 由 SourceDeadline 的闸拒绝（不修正、不放宽）。
        assert_eq!(
            SourceDeadline::new(10, 10).unwrap_err(),
            ExecError::QueryConstraintInvalid
        );
        assert_eq!(
            SourceDeadline::new(-1, 10).unwrap_err(),
            ExecError::QueryConstraintInvalid
        );
        assert!(SourceDeadline::new(0, 1).is_ok());
    }

    #[test]
    fn selected_scope_echo_must_match_typed_request() {
        assert!(check_selected_scope_echo(&project(16), &project(16)).is_ok());
        assert_eq!(
            check_selected_scope_echo(&project(16), &SelectedScope::AllAuthorized).unwrap_err(),
            ExecError::QueryConstraintInvalid
        );
        assert_eq!(
            check_selected_scope_echo(&SelectedScope::AllAuthorized, &project(16)).unwrap_err(),
            ExecError::QueryConstraintInvalid
        );
        assert_eq!(
            check_selected_scope_echo(&project(16), &project(17)).unwrap_err(),
            ExecError::QueryConstraintInvalid
        );
    }

    #[test]
    fn action_and_subject_checks_reject_forgery() {
        let action: RequestAction = serde_json::from_value(json!({
            "method": "get",
            "path": "/api/deployment/task/search",
            "route_contract_version": "deployment-project-transition-1",
        }))
        .unwrap();
        assert!(check_action(&action, &action).is_ok());
        let mut other = action.clone();
        other.path = "/api/deployment/task/detail".to_owned();
        assert_eq!(
            check_action(&action, &other).unwrap_err(),
            ExecError::ActionMismatch
        );
        let subject: RequestSubject = serde_json::from_value(json!({
            "tenant_id": "4", "user_id": "7", "tenant_type": "team"
        }))
        .unwrap();
        let identity = EnvelopeIdentity::from_parts(Some(4), Some(7), Some("team")).unwrap();
        assert!(check_subject(&identity, &subject).is_ok());
        assert_eq!(
            check_subject(&identity, &serde_json::from_value(json!({
                "tenant_id": "5", "user_id": "7", "tenant_type": "team"
            })).unwrap()).unwrap_err(),
            ExecError::SubjectMismatch
        );
        assert_eq!(
            check_subject(&identity, &serde_json::from_value(json!({
                "tenant_id": "4", "user_id": "8", "tenant_type": "team"
            })).unwrap()).unwrap_err(),
            ExecError::SubjectMismatch
        );
        assert!(EnvelopeIdentity::from_parts(Some(0), None, None).is_none());
        assert!(EnvelopeIdentity::from_parts(None, None, None).is_none());
    }
}
