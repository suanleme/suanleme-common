//! Explicit **C1 transition protocol** envelope (`schema = 1`, `protocol = 1`).
//!
//! ## What this is — and what it is not
//!
//! This module carries the *project / object authorization execution context* over the
//! **existing C1 token pair** (`tree_token`, `tenant_token`, read from
//! `Iam:Token:tree` / `Iam:Token:tenant:{tenant_id}`) plus the **C2 `SourceLease`**
//! (`source_started_at_unix_ms` / `source_expires_at_unix_ms`, an absolute source
//! deadline, never a fresh TTL). It is a deliberately explicit **transition**
//! protocol while the full V2 shared-token (`epoch` / persistent tokens) rollout has
//! not been obtained.
//!
//! It is **not** [`crate::authz::ExecutionContext`] (the V2 wire type):
//!
//! | | V2 [`crate::authz::ExecutionContext`] | this transition context |
//! |---|---|---|
//! | `protocol` | `2` | `1` |
//! | authority generation | `version: AuthVersion` (`cache_protocol = 2`, `auth_epoch` UUID) | `auth_state: TransitionAuthState` (C1 `tree_token` / `tenant_token`) |
//! | source deadline | `lease: SourceLease` | same two absolute millisecond fields inside `auth_state` |
//! | query condition | `QueryConstraint` (includes `version` + `lease`) | [`TransitionQueryConstraint`] (same condition fields + `auth_state`, no V2 version) |
//!
//! No `epoch` / `cache_protocol = 2` field exists here, and none may be fabricated:
//! the transition protocol must be identifiable from the wire alone, so consumers can
//! refuse to mix it with V2 values/caches.
//!
//! ## Replacement condition
//!
//! Replace this protocol (and [`crate::business::CommonRequest::auth_context`]'s
//! transition variant) with the V2 types only after the V2 epoch/persistent-token
//! rollout actually exists and both the gateway and every business guard consume it.
//! Until then any cache entry, request digest or context produced under this schema is
//! scoped to schema 1 and must not be treated as V2 (and vice versa).
//!
//! ## Trust
//!
//! Deserialization validates the wire *shape and ranges* only. It does **not**
//! authenticate the sender, prove the source lease is still valid, prove the digest
//! matches the actual business input, or prove the targets describe real objects.
//! Consumers (business guards) must check audience/route, digests, source deadline,
//! and the real object ownership themselves.

use serde::{Deserialize, Serialize};
use std::fmt;

use crate::authz::{
    AuthorizedTarget, Clause, RequestAction, RequestSubject, SelectedScope, UuidText,
};

/// Wire protocol discriminator for this transition envelope.
pub const TRANSITION_PROTOCOL: u16 = 1;
/// Schema discriminator for [`TransitionAuthState`].
pub const TRANSITION_AUTH_STATE_SCHEMA: u16 = 1;
/// Largest integer that survives an IEEE-754 double losslessly (2^53 - 1).
const MAX_SAFE_INTEGER: i64 = 9_007_199_254_740_991;

/// C1 token pair + fixed absolute source deadline.
///
/// `schema` is an explicit wire value and must be exactly
/// [`TRANSITION_AUTH_STATE_SCHEMA`] (1); a missing or unknown schema never defaults in.
/// `tree_token` / `tenant_token` are the existing C1 counters (nonnegative i64, no
/// wrap, no local `1` default). The source times are absolute UTC Unix milliseconds:
/// `source_started_at_unix_ms <= source_expires_at_unix_ms <= 2^53 - 1`; equality
/// represents the already-expired boundary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "TransitionAuthStateWire", deny_unknown_fields)]
pub struct TransitionAuthState {
    pub schema: u16,
    pub tree_token: i64,
    pub tenant_token: i64,
    pub source_started_at_unix_ms: i64,
    pub source_expires_at_unix_ms: i64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TransitionAuthStateWire {
    schema: u16,
    tree_token: i64,
    tenant_token: i64,
    source_started_at_unix_ms: i64,
    source_expires_at_unix_ms: i64,
}

impl TransitionAuthState {
    /// Build a validated state. `schema` is fixed to 1 by this constructor.
    pub fn new(
        tree_token: i64,
        tenant_token: i64,
        source_started_at_unix_ms: i64,
        source_expires_at_unix_ms: i64,
    ) -> Result<Self, &'static str> {
        let state = Self {
            schema: TRANSITION_AUTH_STATE_SCHEMA,
            tree_token,
            tenant_token,
            source_started_at_unix_ms,
            source_expires_at_unix_ms,
        };
        state.validate()?;
        Ok(state)
    }

    /// Validate schema, token ranges and the absolute source time range.
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.schema != TRANSITION_AUTH_STATE_SCHEMA {
            return Err("unsupported transition auth-state schema");
        }
        if self.tree_token < 0 || self.tenant_token < 0 {
            return Err("transition tokens must be nonnegative");
        }
        if self.source_started_at_unix_ms < 0
            || self.source_expires_at_unix_ms < self.source_started_at_unix_ms
            || self.source_expires_at_unix_ms > MAX_SAFE_INTEGER
        {
            return Err("invalid transition source time range");
        }
        Ok(())
    }

    /// Half-open deadline: at or after `source_expires_at_unix_ms` the state is expired.
    /// Consumers must still subtract their clock-skew budget; this is not that budget.
    pub fn is_expired(&self, now_unix_ms: i64) -> bool {
        now_unix_ms >= self.source_expires_at_unix_ms
    }
}

impl TryFrom<TransitionAuthStateWire> for TransitionAuthState {
    type Error = &'static str;
    fn try_from(value: TransitionAuthStateWire) -> Result<Self, Self::Error> {
        let state = Self {
            schema: value.schema,
            tree_token: value.tree_token,
            tenant_token: value.tenant_token,
            source_started_at_unix_ms: value.source_started_at_unix_ms,
            source_expires_at_unix_ms: value.source_expires_at_unix_ms,
        };
        state.validate()?;
        Ok(state)
    }
}

/// Query condition compiled under the transition protocol.
///
/// Same condition semantics as [`crate::authz::QueryConstraint`] (selected scope,
/// `public_bypass`, paired allow/deny clauses carrying scope + resource patterns) but
/// bound to the C1 [`TransitionAuthState`] instead of a V2 `AuthVersion`/`SourceLease`.
/// Clients may not send this type; the PDP response is the only source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "TransitionQueryConstraintWire", deny_unknown_fields)]
pub struct TransitionQueryConstraint {
    pub domain: String,
    pub selected_scope: SelectedScope,
    pub public_bypass: bool,
    pub allow_clauses: Vec<Clause>,
    pub deny_clauses: Vec<Clause>,
    pub auth_state: TransitionAuthState,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TransitionQueryConstraintWire {
    domain: String,
    selected_scope: SelectedScope,
    public_bypass: bool,
    allow_clauses: Vec<Clause>,
    deny_clauses: Vec<Clause>,
    auth_state: TransitionAuthState,
}

impl TransitionQueryConstraint {
    /// Validate the embedded auth state. Missing allow/deny entries are meaningful
    /// (empty allow = empty set, empty deny = false) and are not rejected here.
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.domain.trim().is_empty() {
            return Err("transition query domain must be non-empty");
        }
        self.auth_state.validate()
    }
}

impl TryFrom<TransitionQueryConstraintWire> for TransitionQueryConstraint {
    type Error = &'static str;
    fn try_from(value: TransitionQueryConstraintWire) -> Result<Self, Self::Error> {
        let constraint = Self {
            domain: value.domain,
            selected_scope: value.selected_scope,
            public_bypass: value.public_bypass,
            allow_clauses: value.allow_clauses,
            deny_clauses: value.deny_clauses,
            auth_state: value.auth_state,
        };
        constraint.validate()?;
        Ok(constraint)
    }
}

/// Trusted execution context injected by the gateway for this transition package.
///
/// Field-by-field mirror of [`crate::authz::ExecutionContext`] except:
/// * `protocol` is the explicit transition value [`TRANSITION_PROTOCOL`] (1), not the V2
///   `ProtocolV2` (2);
/// * `version`/`lease` are replaced by [`TransitionAuthState`] (C1 token pair + C2
///   absolute source deadline);
/// * `query_constraint` is [`TransitionQueryConstraint`] (schema 1), not the V2 one.
///
/// `targets` are the already-authorized IAM targets (see `data-model` §2); a new-root
/// create target carries `NewRoot`, a resolved object target carries `Existing`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "TransitionExecutionContextWire", deny_unknown_fields)]
pub struct TransitionExecutionContext {
    pub protocol: u16,
    pub request_id: UuidText,
    pub audience: String,
    pub subject: RequestSubject,
    pub action: RequestAction,
    pub request_digest: String,
    pub targets_digest: String,
    pub targets: Vec<AuthorizedTarget>,
    pub query_constraint: Option<TransitionQueryConstraint>,
    pub auth_state: TransitionAuthState,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TransitionExecutionContextWire {
    protocol: u16,
    request_id: UuidText,
    audience: String,
    subject: RequestSubject,
    action: RequestAction,
    request_digest: String,
    targets_digest: String,
    targets: Vec<AuthorizedTarget>,
    query_constraint: Option<TransitionQueryConstraint>,
    auth_state: TransitionAuthState,
}

impl TransitionExecutionContext {
    /// Validate protocol (semantic) plus the wire shape.
    ///
    /// Protocol support is a **semantic** decision: it must be enforced by consumers such as
    /// the deployment guard (`GuardError::ProtocolUnsupported`) so that a wire-format valid
    /// envelope with `protocol != 1` produces an explicit protocol error instead of a
    /// framework deserialization error. Wire parsing therefore only runs
    /// [`Self::validate_shape`]; callers that execute the envelope must call this method.
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.protocol != TRANSITION_PROTOCOL {
            return Err("unsupported transition protocol");
        }
        self.validate_shape()
    }

    /// Validate only wire format / field shape (no protocol-support decision).
    fn validate_shape(&self) -> Result<(), &'static str> {
        if self.audience.trim().is_empty() {
            return Err("transition audience must be non-empty");
        }
        if self.subject.tenant_type.trim().is_empty() {
            return Err("transition subject tenant_type must be non-empty");
        }
        if !is_sha256_hex(&self.request_digest) {
            return Err("transition request digest must be 64 lowercase hex characters");
        }
        if !is_sha256_hex(&self.targets_digest) {
            return Err("transition targets digest must be 64 lowercase hex characters");
        }
        self.auth_state.validate()?;
        if let Some(constraint) = &self.query_constraint {
            constraint.validate()?;
        }
        Ok(())
    }

    /// Half-open source deadline of the envelope itself.
    pub fn is_expired(&self, now_unix_ms: i64) -> bool {
        self.auth_state.is_expired(now_unix_ms)
    }
}

impl TryFrom<TransitionExecutionContextWire> for TransitionExecutionContext {
    type Error = &'static str;
    fn try_from(value: TransitionExecutionContextWire) -> Result<Self, Self::Error> {
        let context = Self {
            protocol: value.protocol,
            request_id: value.request_id,
            audience: value.audience,
            subject: value.subject,
            action: value.action,
            request_digest: value.request_digest,
            targets_digest: value.targets_digest,
            targets: value.targets,
            query_constraint: value.query_constraint,
            auth_state: value.auth_state,
        };
        // 线格式/字段形状在此拒绝；protocol 语义由 `validate()`/guard 拒绝（C002/protocol_unsupported）。
        context.validate_shape()?;
        Ok(context)
    }
}

impl fmt::Display for TransitionAuthState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "schema={} tree={} tenant={} source=[{},{}]",
            self.schema,
            self.tree_token,
            self.tenant_token,
            self.source_started_at_unix_ms,
            self.source_expires_at_unix_ms
        )
    }
}

/// The transition envelope is the only value carried by
/// [`crate::business::CommonRequest::auth_context`] in this package: a V2
/// [`crate::authz::ExecutionContext`] must never be silently coerced into it.
impl From<TransitionExecutionContext> for serde_json::Value {
    fn from(value: TransitionExecutionContext) -> Self {
        // Serialization of the validated struct cannot fail.
        serde_json::to_value(value).unwrap_or(serde_json::Value::Null)
    }
}

/// Helper: extract the transition context injected in a `CommonRequest`, if any.
///
/// This does not authenticate anything; it only avoids every consumer re-implementing
/// the (currently deliberately failing) conversion from the V2-typed field. The
/// gateway injects via raw JSON, services must parse their own `CommonRequest` with
/// the transition envelope.
pub fn transition_context_from_json(
    value: &serde_json::Value,
) -> Result<TransitionExecutionContext, &'static str> {
    serde_json::from_value(value.clone()).map_err(|_| "invalid transition execution context")
}

fn is_sha256_hex(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn state_json() -> serde_json::Value {
        json!({
            "schema": 1,
            "tree_token": 12,
            "tenant_token": 34,
            "source_started_at_unix_ms": 1_800_000_000_000_i64,
            "source_expires_at_unix_ms": 1_800_000_060_000_i64
        })
    }

    fn target_json(role: &str, object_id: &str) -> serde_json::Value {
        json!({
            "target_kind": "existing",
            "target_key": "a".repeat(64),
            "target_role": role,
            "action": {"method":"get","path":"/api/deployment/task/detail","route_contract_version":"deployment-project-transition-1"},
            "stamp": {
                "provider":"deployment","object_type":"task","object_id":object_id,"root_id":object_id,
                "mapping_version":"deployment-project-transition-1",
                "project":{"kind":"project","project_id":"16"},
                "resources":[],
                "physical_identities":[]
            }
        })
    }

    fn context_json() -> serde_json::Value {
        json!({
            "protocol": 1,
            "request_id": "11111111-1111-4111-8111-111111111111",
            "audience": "gj-deployment-api",
            "subject": {"tenant_id":"4","user_id":"7","tenant_type":"team"},
            "action": {"method":"get","path":"/api/deployment/task/detail","route_contract_version":"deployment-project-transition-1"},
            "request_digest": "b".repeat(64),
            "targets_digest": "c".repeat(64),
            "targets": [target_json("primary", "123")],
            "query_constraint": null,
            "auth_state": state_json()
        })
    }

    #[test]
    fn transition_context_roundtrips_exactly() {
        let original = context_json();
        let parsed: TransitionExecutionContext = serde_json::from_value(original.clone()).unwrap();
        assert_eq!(serde_json::to_value(parsed).unwrap(), original);
        assert_eq!(TRANSITION_PROTOCOL, 1);
        assert_eq!(TRANSITION_AUTH_STATE_SCHEMA, 1);
    }

    #[test]
    fn transition_context_rejects_v2_or_unknown_security_fields() {
        // The transition protocol must never smuggle a V2 epoch/cache protocol.
        for (field, value) in [
            ("epoch", json!("22222222-2222-4222-8222-222222222222")),
            ("cache_protocol", json!(2)),
            ("version", json!({"cache_protocol":2})),
            ("lease", json!({"source_started_at_unix_ms":1,"source_expires_at_unix_ms":2})),
            ("scope_all", json!(true)),
        ] {
            let mut candidate = context_json();
            candidate[field] = value;
            assert!(
                serde_json::from_value::<TransitionExecutionContext>(candidate).is_err(),
                "unknown top-level field {field} must be rejected"
            );
        }
        // ... and the same for the embedded query constraint / auth state.
        let mut candidate = context_json();
        candidate["auth_state"]["lease_format"] = json!("iam-c1-source-lease-v1");
        assert!(serde_json::from_value::<TransitionExecutionContext>(candidate).is_err());
        let mut candidate = context_json();
        candidate["auth_state"]["schema"] = json!(2);
        assert!(serde_json::from_value::<TransitionExecutionContext>(candidate).is_err());
    }

    #[test]
    fn transition_context_requires_every_security_field() {
        for field in [
            "protocol",
            "request_id",
            "audience",
            "subject",
            "action",
            "request_digest",
            "targets_digest",
            "targets",
            "auth_state",
        ] {
            let mut candidate = context_json();
            candidate.as_object_mut().unwrap().remove(field);
            assert!(
                serde_json::from_value::<TransitionExecutionContext>(candidate).is_err(),
                "missing {field}"
            );
        }
        for missing_optional in ["query_constraint"] {
            let mut candidate = context_json();
            candidate.as_object_mut().unwrap().remove(missing_optional);
            let parsed: TransitionExecutionContext =
                serde_json::from_value(candidate).expect("optional field may be absent");
            assert!(parsed.query_constraint.is_none());
        }
        for field in [
            "schema",
            "tree_token",
            "tenant_token",
            "source_started_at_unix_ms",
            "source_expires_at_unix_ms",
        ] {
            let mut candidate = context_json();
            candidate["auth_state"].as_object_mut().unwrap().remove(field);
            assert!(
                serde_json::from_value::<TransitionExecutionContext>(candidate).is_err(),
                "missing auth_state.{field}"
            );
        }
    }

    #[test]
    fn auth_state_ranges_are_half_open_and_bounded() {
        for invalid in [
            json!({"schema":1,"tree_token":-1,"tenant_token":0,"source_started_at_unix_ms":0,"source_expires_at_unix_ms":1}),
            json!({"schema":1,"tree_token":0,"tenant_token":-1,"source_started_at_unix_ms":0,"source_expires_at_unix_ms":1}),
            json!({"schema":1,"tree_token":0,"tenant_token":0,"source_started_at_unix_ms":-1,"source_expires_at_unix_ms":1}),
            json!({"schema":1,"tree_token":0,"tenant_token":0,"source_started_at_unix_ms":5,"source_expires_at_unix_ms":4}),
            json!({"schema":1,"tree_token":0,"tenant_token":0,"source_started_at_unix_ms":0,"source_expires_at_unix_ms":9_007_199_254_740_992_i64}),
            json!({"schema":0,"tree_token":0,"tenant_token":0,"source_started_at_unix_ms":0,"source_expires_at_unix_ms":1}),
            json!({"schema":1,"tree_token":0,"tenant_token":0,"source_started_at_unix_ms":0,"source_expires_at_unix_ms":1,"extra":true}),
        ] {
            assert!(
                serde_json::from_value::<TransitionAuthState>(invalid.clone()).is_err(),
                "{invalid}"
            );
        }
        let state: TransitionAuthState = serde_json::from_value(state_json()).unwrap();
        assert!(!state.is_expired(state.source_started_at_unix_ms));
        assert!(!state.is_expired(state.source_expires_at_unix_ms - 1));
        assert!(state.is_expired(state.source_expires_at_unix_ms));
        assert!(state.is_expired(state.source_expires_at_unix_ms + 1));
        // Exactly the same start/expiry is the documented expired boundary.
        let boundary = TransitionAuthState::new(0, 0, 7, 7).unwrap();
        assert!(boundary.is_expired(7));
        assert!(TransitionAuthState::new(0, 0, 8, 7).is_err());
        assert!(TransitionAuthState::new(
            0,
            0,
            0,
            MAX_SAFE_INTEGER
        )
        .is_ok());
    }

    #[test]
    fn query_constraint_carries_auth_state_and_rejects_v2_fields() {
        let value = json!({
            "domain": "deployment-project-transition-1",
            "selected_scope": {"kind":"all_authorized"},
            "public_bypass": false,
            "allow_clauses": [],
            "deny_clauses": [{
                "scope": {"kind":"project","project_id":"16"},
                "resource_patterns": [{"kind":"whole_segment_star","resource_arn":"gjsl:deployment:*:4:task/*"}]
            }],
            "auth_state": state_json()
        });
        let parsed: TransitionQueryConstraint = serde_json::from_value(value.clone()).unwrap();
        assert!(!parsed.public_bypass);
        assert!(parsed.allow_clauses.is_empty());
        assert_eq!(serde_json::to_value(&parsed).unwrap(), value);
        for field in ["selected_scope", "public_bypass", "allow_clauses", "deny_clauses", "auth_state"] {
            let mut candidate = value.clone();
            candidate.as_object_mut().unwrap().remove(field);
            assert!(
                serde_json::from_value::<TransitionQueryConstraint>(candidate).is_err(),
                "missing {field}"
            );
        }
        for (field, payload) in [
            ("version", json!({"cache_protocol":2})),
            ("lease", json!({"source_started_at_unix_ms":1,"source_expires_at_unix_ms":2})),
            ("epoch", json!("22222222-2222-4222-8222-222222222222")),
        ] {
            let mut candidate = value.clone();
            candidate[field] = payload;
            assert!(serde_json::from_value::<TransitionQueryConstraint>(candidate).is_err());
        }
        let mut candidate = value.clone();
        candidate["domain"] = json!("   ");
        assert!(serde_json::from_value::<TransitionQueryConstraint>(candidate).is_err());
    }

    #[test]
    fn execution_context_validates_digest_shape_and_audience() {
        for (pointer, payload) in [
            ("/request_digest", json!("not-a-digest")),
            ("/targets_digest", json!("A".repeat(64))),
            ("/audience", json!("")),
            ("/subject/tenant_type", json!(" ")),
        ] {
            let mut candidate = context_json();
            *candidate.pointer_mut(pointer).unwrap() = payload;
            assert!(
                serde_json::from_value::<TransitionExecutionContext>(candidate).is_err(),
                "{pointer}"
            );
        }
        let context: TransitionExecutionContext =
            serde_json::from_value(context_json()).unwrap();
        assert_eq!(context.protocol, TRANSITION_PROTOCOL);
        assert!(context.query_constraint.is_none());
        let expires = context.auth_state.source_expires_at_unix_ms;
        assert!(!context.is_expired(expires - 1));
        assert!(context.is_expired(expires));
    }

    #[test]
    fn transition_context_accepts_query_mode_envelope() {
        let mut value = context_json();
        value["targets"] = json!([]);
        value["query_constraint"] = json!({
            "domain": "deployment-project-transition-1",
            "selected_scope": {"kind":"project","project_id":"16"},
            "public_bypass": false,
            "allow_clauses": [],
            "deny_clauses": [],
            "auth_state": state_json()
        });
        let context: TransitionExecutionContext = serde_json::from_value(value).unwrap();
        assert!(context.targets.is_empty());
        let constraint = context.query_constraint.unwrap();
        assert_eq!(constraint.auth_state, context.auth_state);
    }

    #[test]
    fn transition_protocol_is_enforced_by_validation_not_wire_parsing() {
        // FE-S2-021 修复（PLAN-FIX-01）：protocol != 1 必须可完成线格式反序列化，
        // 由 validate()/guard 返回协议不支持（C002/protocol_unsupported），而不是解析期 500。
        let mut candidate = context_json();
        candidate["protocol"] = json!(2);
        let parsed: TransitionExecutionContext =
            serde_json::from_value(candidate).expect("protocol=2 must be wire-shape valid");
        assert_eq!(parsed.protocol, 2);
        assert_eq!(parsed.validate(), Err("unsupported transition protocol"));

        // protocol=1 行为不变：可解析且 validate 通过
        let ok: TransitionExecutionContext = serde_json::from_value(context_json()).unwrap();
        assert_eq!(ok.protocol, TRANSITION_PROTOCOL);
        assert!(ok.validate().is_ok());

        // 未知字段仍拒绝（deny_unknown_fields 不放宽）
        let mut unknown = context_json();
        unknown["epoch"] = json!("22222222-2222-4222-8222-222222222222");
        assert!(serde_json::from_value::<TransitionExecutionContext>(unknown).is_err());
    }

    #[test]
    fn transition_context_can_be_read_from_json_value() {
        let value = context_json();
        let context = transition_context_from_json(&value).unwrap();
        assert_eq!(context.audience, "gj-deployment-api");
        assert!(transition_context_from_json(&json!({"protocol":1})).is_err());
        let mut v2ish = value;
        v2ish["protocol"] = json!(2);
        // 线格式可解析（协议支持是语义判定，不在反序列化阶段拒绝）
        let parsed = transition_context_from_json(&v2ish).expect("protocol=2 is wire-shape valid");
        assert_eq!(parsed.protocol, 2);
        assert_eq!(parsed.validate(), Err("unsupported transition protocol"));
    }
}
