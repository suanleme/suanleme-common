//! **授权整单撤销的共享规则单点**（REQ-14 `D330`：UC 成员移出/用户组级联删除、IAM 撤权核心共同使用）。
//!
//! ## 为什么放在 `suanleme-common`
//!
//! UC 与 IAM 部署在**同一个权威身份库**（unified identity：`authz` / `authorization_policy` /
//! `policy` / `audit_change` 与 `tenant` / `tenant_user` / `tenant_member_instance` / `user_group`
//! 同库）。REQ-14 要求成员移出、用户组删除在 **UC 自己的业务事务内**直接完成撤销与审计，
//! 不允许跨服务 RPC 声称共享事务；IAM 已有唯一删除写核心
//! （`gj-iam-center/src/service/authorization.rs::delete_located_in_tx`）。
//!
//! 为避免 UC 另写第二套撤权规则，把**对两侧都必须是同一形状**的部分收敛到这里：
//! 锁 key 构造、锁 SQL、候选行/行锁/策略快照/撤单 SQL、撤销审计 JSON 快照、审计动作常量、
//! 以及锁序说明。本模块**不依赖 sqlx**（只有 `serde_json`），由各服务用自己的连接执行。
//!
//! ## 锁序（与 IAM `delete_located_in_tx` / UC 成员写方逐项一致）
//!
//! ```text
//! tenant 行（IAM 撤权 FOR SHARE；UC 成员/组写事务已先取 FOR UPDATE，更强且顺序相同）
//!   → 主体生命周期行（user ⇒ tenant_member_instance 周期行 FOR UPDATE；
//!                     user_group ⇒ user_group 行 FOR UPDATE）
//!   → pg_advisory_xact_lock('iam-authz:{tenant}:{subject_type}:{binding_id}:{scope_or_0}')
//!   → authz 行 FOR UPDATE
//! ```
//!
//! 主体绑定 id（`binding_id`）是**真实绑定列**：user ⇒ `subject_tenant_user_id`
//! （成员**周期** id，不是平台 user_id）；user_group ⇒ `subject_user_group_id`。
//! 逐单撤销时按 `authz.id` **升序**取 advisory/行锁（与 IAM 单行路径无反向环；
//! 同主体并发写由组/周期行锁串行化）。
//!
//! ## 审计形状（`audit_change`，与 IAM `datasource/mapper/audit_change.rs::authz_state` 逐字一致）
//!
//! 每撤销一单写一条 `audit_change`：`action = authz_delete`、`target_type = authz`、
//! `target_id = authz.id`、`subject_type = user|user_group`、
//! `subject_id = 该主体的对外 id（user ⇒ 平台 user_id，user_group ⇒ 组 id）`、
//! `scope_resource_group_id = 该单 scope`、`before_json = authz_state(Some(id), …, 撤销前 policy_ids)`、
//! `after_json = authz_state(None, …, [])`、`source = api`。
//!
//! **`subject_id` 语义刻意与 IAM 现有行保持一致**（user 主体是平台 user_id，不是成员周期 id）：
//! 同一 `action+target_type` 的审计行必须只有一种主体语义，否则两表/两服务合并出的变更史不可读。
//! 成员**周期 id** 由撤销方写进 `detail` 文本（UC 级联），不改 JSON 形状、不偷换旧列含义
//! （data-model §3.6 / §5 的口径）。
//!
//! **版本**：本模块随第一次使用者（REQ-14 RID `req14-e16660ef`）落地；未发布 registry，
//! 使用方以本地 path patch 引用（见各仓 Cargo.toml 的 `[patch.suanleme]`）。

use serde_json::{json, Value};

/// 撤销审计的动作（与 IAM `audit_change::ACTION_AUTHZ_DELETE` 同值）。
pub const ACTION_AUTHZ_DELETE: &str = "authz_delete";
/// 撤销审计的目标类型（与 IAM `audit_change::TARGET_AUTHZ` 同值）。
pub const TARGET_AUTHZ: &str = "authz";
/// 撤销审计的来源（控制面 API 写，与 IAM `audit_change::SOURCE_API` 同值）。
pub const SOURCE_API: &str = "api";

/// 授权主体的两种类型（`authz.subject_type` 的 CHECK 取值）。
pub const SUBJECT_USER: &str = "user";
/// 授权主体的两种类型（`authz.subject_type` 的 CHECK 取值）。
pub const SUBJECT_USER_GROUP: &str = "user_group";

/// 锁序的唯一文字口径（日志/报告/测试引用；不参与 SQL）。
pub const LOCK_ORDER: &str = "tenant -> subject lifecycle row (tenant_member_instance|user_group FOR UPDATE) -> pg_advisory_xact_lock(iam-authz identity) -> authz row FOR UPDATE";

/// 租户行锁（IAM 撤权核心的 `lock_authorization_tenant` 同一 SQL）。
///
/// UC 的成员/组写事务通常已先对 tenant 取更强的 `FOR UPDATE`；同一事务内再取 `FOR SHARE`
/// 不会反向（PostgreSQL 行锁强度只升不降），锁序仍然成立。
pub const TENANT_LOCK_SQL: &str = "select id from tenant where id = $1 for share";

/// 事务级 advisory 身份锁 SQL（参数为 [`identity_lock_key`] 构造的 key）。
pub const IDENTITY_LOCK_SQL: &str = "select pg_advisory_xact_lock(hashtext($1)::bigint)";

/// user 主体的生命周期行锁：`tenant_member_instance` 的**周期行**（含已结束周期）。
///
/// 与 IAM `lock_current_tenant_user`（写准入）/`lock_authorization_subject_lifecycle`（删除）
/// 使用同一物理行；不锁 `tenant_user` 行（原成员行退出即删，反向锁会与 UC 成环）。
pub const SUBJECT_LIFECYCLE_USER_SQL: &str =
    "select id from tenant_member_instance where tenant_id = $1 and id = $2 for update";

/// user_group 主体的生命周期行锁：`user_group` 行（不看状态，停用组授权仍可撤销）。
pub const SUBJECT_LIFECYCLE_GROUP_SQL: &str =
    "select id from user_group where tenant_id = $1 and id = $2 for update";

/// 取主体生命周期行锁 SQL（按 `subject_type` 选 statement；与 IAM 同一对 SQL）。
pub fn subject_lifecycle_lock_sql(subject_type: &str) -> &'static str {
    match subject_type {
        SUBJECT_USER_GROUP => SUBJECT_LIFECYCLE_GROUP_SQL,
        _ => SUBJECT_LIFECYCLE_USER_SQL,
    }
}

/// 成员移出候选授权单（**任何 status**，只按**成员周期 id**，绝不按平台 user_id 扫）。
///
/// 返回 `(authorization_id, scope, subject_id=平台 user_id, binding_id=周期 id)`；
/// `order by a.id` 是后续逐单加锁的稳定顺序。平台 user_id 只用于审计快照（`subject_id` 口径），
/// 不参与筛选谓词；`binding_id` 是 advisory 身份锁/行锁复查用的真实绑定列。
pub const AUTHZ_IDS_BY_MEMBER_INSTANCES_SQL: &str = "select a.id, a.scope_resource_group_id, \
            tmi.user_id as subject_id, a.subject_tenant_user_id as binding_id \
       from authz a \
       join tenant_member_instance tmi \
         on tmi.tenant_id = a.tenant_id \
        and tmi.id = a.subject_tenant_user_id \
      where a.tenant_id = $1 \
        and a.subject_type = 'user' \
        and a.subject_tenant_user_id = any($2) \
      order by a.id";

/// 用户组级联删除候选授权单（该组主体**全部 status**，只按组 id）。
///
/// 返回 `(authorization_id, scope, subject_id=组 id, binding_id=组 id)`；`order by a.id` 稳定加锁顺序。
pub const AUTHZ_IDS_BY_USER_GROUP_SQL: &str = "select a.id, a.scope_resource_group_id, \
            a.subject_user_group_id as subject_id, a.subject_user_group_id as binding_id \
       from authz a \
      where a.tenant_id = $1 \
        and a.subject_type = 'user_group' \
        and a.subject_user_group_id = $2 \
      order by a.id";

/// 授权行锁 + 锁后重读（参数：`tenant_id, authorization_id`）。
///
/// 只取撤销需要的最小列；调用方用读到的绑定列复查"仍是要撤的那一单"，不信任锁前快照。
pub const AUTHZ_ROW_FOR_UPDATE_SQL: &str =
    "select a.id, a.subject_type, a.subject_tenant_user_id, \
            a.subject_user_group_id, a.scope_resource_group_id \
       from authz a \
      where a.tenant_id = $1 and a.id = $2 \
      for update of a";

/// 撤销前快照该单策略 id（升序；`authorization_policy` 由 FK `on delete cascade` 连带清理）。
pub const AUTHZ_POLICY_IDS_SQL: &str =
    "select policy_id from authorization_policy where authorization_id = $1 order by policy_id";

/// 撤销整单（参数：`authorization_id`）——与 IAM `iam::delete_authorization` 同一语句。
///
/// `authorization_policy` 由 FK `on delete cascade` 清理；不做两段手工删。
pub const DELETE_AUTHZ_SQL: &str = "delete from authz where id = $1";

/// 授权身份锁 key（与 IAM `iam::lock_authorization_identity` **逐字一致**）。
///
/// `subject_binding_id` = 真实绑定行 id：user ⇒ 成员**周期** id；user_group ⇒ 组 id。
/// 全局 scope 用 `0` 占位（`scope.unwrap_or(0)`），与 IAM 相同。
pub fn identity_lock_key(
    tenant_id: i64,
    subject_type: &str,
    subject_binding_id: i64,
    scope: Option<i64>,
) -> String {
    format!(
        "iam-authz:{tenant_id}:{subject_type}:{subject_binding_id}:{}",
        scope.unwrap_or(0)
    )
}

/// 授权单状态快照（`audit_change.before_json / after_json` 的**统一形状**）。
///
/// 与 IAM `datasource/mapper/audit_change.rs::authz_state` 同一实现（本函数是该形状的共享来源）：
/// - `authorization_id = None` 表示该时刻该单不存在（撤销整单的 `after_json`）；
/// - `policy_ids` 统一升序输出；
/// - `subject_id` 是该主体的对外 id（user ⇒ 平台 user_id；user_group ⇒ 组 id），
///   **不是**成员周期 id（周期 id 见 [`AUTHZ_IDS_BY_MEMBER_INSTANCES_SQL`] 注释与撤销方 `detail`）。
pub fn authz_state(
    authorization_id: Option<i64>,
    subject_type: &str,
    subject_id: i64,
    scope_resource_group_id: Option<i64>,
    policy_ids: &[i64],
) -> Value {
    let mut ids = policy_ids.to_vec();
    ids.sort_unstable();
    json!({
        "authorization_id": authorization_id,
        "subject_type": subject_type,
        "subject_id": subject_id,
        "scope_resource_group_id": scope_resource_group_id,
        "policy_ids": ids,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 锁 key 必须与 IAM `iam::lock_authorization_identity` 的格式逐字一致。
    ///
    /// IAM 现实现（`gj-iam-center/src/datasource/mapper/iam.rs::lock_authorization_identity`）：
    /// `format!("iam-authz:{tenant_id}:{subject_type}:{subject_binding_id}:{}", scope.unwrap_or(0))`
    #[test]
    fn identity_lock_key_matches_iam_format() {
        assert_eq!(
            identity_lock_key(7, "user", 42, Some(9)),
            "iam-authz:7:user:42:9"
        );
        assert_eq!(
            identity_lock_key(7, "user", 42, None),
            "iam-authz:7:user:42:0"
        );
        assert_eq!(
            identity_lock_key(7, "user_group", 3, None),
            "iam-authz:7:user_group:3:0"
        );
        // 负数/大数不做特殊处理：与 IAM 的 `{}` 格式化完全相同（不截断、不加引号）。
        assert_eq!(
            identity_lock_key(-1, "user", i64::MAX, Some(-2)),
            format!("iam-authz:-1:user:{}:-2", i64::MAX)
        );
    }

    /// 主体生命周期锁 SQL：user 锁周期行、group 锁组行（与 IAM 同一对语句的语义）。
    #[test]
    fn subject_lifecycle_lock_sql_selects_binding_row() {
        assert!(subject_lifecycle_lock_sql("user").contains("tenant_member_instance"));
        assert!(subject_lifecycle_lock_sql("user").contains("for update"));
        assert!(subject_lifecycle_lock_sql("user_group").contains("user_group"));
        assert!(subject_lifecycle_lock_sql("user_group").contains("for update"));
        // 未知类型按 user 分支（与 IAM `match "user_group" => … _ => user` 的默认分支一致）。
        assert_eq!(
            subject_lifecycle_lock_sql("other"),
            SUBJECT_LIFECYCLE_USER_SQL
        );
    }

    /// 候选授权单 SQL：**只按真实绑定列**筛选（周期 id / 组 id），不按平台 user_id 扫；
    /// 不过滤 status（任何状态都要撤）；`order by a.id` 是逐单加锁的稳定顺序。
    #[test]
    fn candidate_sql_filters_by_binding_column_only_and_orders_by_id() {
        let user = AUTHZ_IDS_BY_MEMBER_INSTANCES_SQL;
        assert!(user.contains("a.subject_tenant_user_id = any($2)"));
        assert!(user.contains("a.subject_type = 'user'"));
        assert!(user.contains("order by a.id"));
        assert!(!user.contains("a.status"), "撤销必须覆盖任何 status");
        assert!(
            !user.contains("a.subject_tenant_user_id = $2 "),
            "必须是数组谓词，不能只取单条"
        );

        let group = AUTHZ_IDS_BY_USER_GROUP_SQL;
        assert!(group.contains("a.subject_user_group_id = $2"));
        assert!(group.contains("a.subject_type = 'user_group'"));
        assert!(group.contains("order by a.id"));
        assert!(!group.contains("a.status"));
    }

    /// 撤销 SQL 与 IAM `iam::delete_authorization` / `iam::authz_policy_ids` 同一语句。
    #[test]
    fn revoke_sql_matches_iam_delete_core() {
        assert_eq!(DELETE_AUTHZ_SQL, "delete from authz where id = $1");
        assert_eq!(
            AUTHZ_POLICY_IDS_SQL,
            "select policy_id from authorization_policy where authorization_id = $1 order by policy_id"
        );
        assert!(AUTHZ_ROW_FOR_UPDATE_SQL.contains("for update of a"));
        assert!(TENANT_LOCK_SQL.contains("for share"));
        assert!(IDENTITY_LOCK_SQL.contains("pg_advisory_xact_lock(hashtext($1)::bigint)"));
    }

    /// `authz_state` 形状必须与 IAM `audit_change::authz_state` 逐键一致（键集/取值/策略排序都钉住）。
    ///
    /// 注：本仓 `serde_json` 未启用 `preserve_order`（与 IAM 相同）⇒ `Value` 的键按字母序序列化，
    /// 且审计列是 JSONB（入库后键序本就规范化）。因此形状断言按 **键集 + 取值**，不按字面键序。
    #[test]
    fn authz_state_matches_iam_shape() {
        let before = authz_state(Some(11), "user", 9001, Some(5), &[3, 1, 2]);
        assert_eq!(
            before,
            json!({
                "authorization_id": 11,
                "subject_type": "user",
                "subject_id": 9001,
                "scope_resource_group_id": 5,
                "policy_ids": [1, 2, 3],
            })
        );
        // 撤销整单的「变更后」：单不存在 + 策略清空。
        let after = authz_state(None, "user", 9001, None, &[]);
        assert_eq!(
            after,
            json!({
                "authorization_id": null,
                "subject_type": "user",
                "subject_id": 9001,
                "scope_resource_group_id": null,
                "policy_ids": [],
            })
        );
        // 键集与 IAM 完全一致（多键/少键都会失败）。
        let keys: Vec<&str> = before
            .as_object()
            .expect("object")
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(
            keys,
            vec![
                "authorization_id",
                "policy_ids",
                "scope_resource_group_id",
                "subject_id",
                "subject_type"
            ]
        );
        // 组主体：subject_id 是组 id。
        let group = authz_state(Some(12), "user_group", 77, None, &[6]);
        assert_eq!(group["subject_type"], "user_group");
        assert_eq!(group["subject_id"], 77);
    }

    /// 审计常量与 IAM 现有值一致（同库同表，不能出现第二套字面量）。
    #[test]
    fn audit_constants_match_iam_values() {
        assert_eq!(ACTION_AUTHZ_DELETE, "authz_delete");
        assert_eq!(TARGET_AUTHZ, "authz");
        assert_eq!(SOURCE_API, "api");
        assert_eq!(SUBJECT_USER, "user");
        assert_eq!(SUBJECT_USER_GROUP, "user_group");
        assert!(LOCK_ORDER.contains("tenant"));
        assert!(LOCK_ORDER.contains("advisory"));
        assert!(LOCK_ORDER.contains("authz row"));
    }
}
