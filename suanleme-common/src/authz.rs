//! V2 authorization transport. These types validate wire shape, not trust.
//!
//! A received context is not a capability: consumers must authenticate its source
//! and check audience, actual subject/action/targets, digests, lease, and business
//! ownership against the registered provider contract. No evaluator lives here.

use serde::{de, Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;

macro_rules! decimal_type {
    ($name:ident, $minimum:expr) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(i64);

        impl $name {
            pub fn new(value: i64) -> Result<Self, &'static str> {
                if value < $minimum {
                    Err("decimal value outside allowed range")
                } else {
                    Ok(Self(value))
                }
            }
            pub fn get(self) -> i64 {
                self.0
            }
        }
        impl TryFrom<i64> for $name {
            type Error = &'static str;
            fn try_from(value: i64) -> Result<Self, Self::Error> {
                Self::new(value)
            }
        }
        impl Serialize for $name {
            fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                serializer.serialize_str(&self.0.to_string())
            }
        }
        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                struct DecimalVisitor;
                impl de::Visitor<'_> for DecimalVisitor {
                    type Value = $name;
                    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                        f.write_str("a canonical decimal string within the allowed i64 range")
                    }
                    fn visit_str<E: de::Error>(self, value: &str) -> Result<Self::Value, E> {
                        if value.is_empty()
                            || !value.bytes().all(|byte| byte.is_ascii_digit())
                            || (value.len() > 1 && value.starts_with('0'))
                        {
                            return Err(E::custom("invalid canonical decimal string"));
                        }
                        let parsed = value
                            .parse::<i64>()
                            .map_err(|_| E::custom("decimal exceeds i64"))?;
                        $name::new(parsed).map_err(E::custom)
                    }
                }
                deserializer.deserialize_str(DecimalVisitor)
            }
        }
    };
}

// New V2 database ID; existing legacy request IDs keep their old numeric format.
decimal_type!(Id, 1);
// V2 token; no wrap, default, local fallback, or floating-point conversion.
decimal_type!(Token, 0);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct UuidText(String);
impl UuidText {
    pub fn new(value: impl Into<String>) -> Result<Self, &'static str> {
        let value = value.into();
        let parsed = uuid::Uuid::parse_str(&value).map_err(|_| "invalid UUID")?;
        if parsed.hyphenated().to_string() != value {
            return Err("UUID must be canonical lowercase hyphenated text");
        }
        Ok(Self(value))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl Serialize for UuidText {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}
impl<'de> Deserialize<'de> for UuidText {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::new(String::deserialize(deserializer)?).map_err(de::Error::custom)
    }
}

/// Required explicit wire value 2; a missing/unknown protocol never defaults in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProtocolV2;
impl Serialize for ProtocolV2 {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u16(2)
    }
}
impl<'de> Deserialize<'de> for ProtocolV2 {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        if u16::deserialize(deserializer)? == 2 {
            Ok(Self)
        } else {
            Err(de::Error::custom("unsupported authorization protocol"))
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthVersion {
    pub cache_protocol: ProtocolV2,
    pub auth_epoch: UuidText,
    pub tree_token: Token,
    pub tenant_token: Token,
}

/// Absolute source times, never a fresh TTL. Clock/staleness budgets are checked
/// by consumers; equal start/expiry represents the documented expired boundary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "LeaseWire", deny_unknown_fields)]
pub struct SourceLease {
    pub source_started_at_unix_ms: i64,
    pub source_expires_at_unix_ms: i64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LeaseWire {
    source_started_at_unix_ms: i64,
    source_expires_at_unix_ms: i64,
}
impl TryFrom<LeaseWire> for SourceLease {
    type Error = &'static str;
    fn try_from(value: LeaseWire) -> Result<Self, Self::Error> {
        const MAX_SAFE_INTEGER: i64 = 9_007_199_254_740_991;
        if value.source_started_at_unix_ms < 0
            || value.source_expires_at_unix_ms < value.source_started_at_unix_ms
            || value.source_expires_at_unix_ms > MAX_SAFE_INTEGER
        {
            return Err("invalid source lease range");
        }
        Ok(Self {
            source_started_at_unix_ms: value.source_started_at_unix_ms,
            source_expires_at_unix_ms: value.source_expires_at_unix_ms,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RequestSubject {
    pub tenant_id: Id,
    pub user_id: Id,
    pub tenant_type: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "ActionWire", deny_unknown_fields)]
pub struct RequestAction {
    pub method: String,
    pub path: String,
    pub route_contract_version: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ActionWire {
    method: String,
    path: String,
    route_contract_version: String,
}
impl TryFrom<ActionWire> for RequestAction {
    type Error = &'static str;
    fn try_from(value: ActionWire) -> Result<Self, Self::Error> {
        if value.method.is_empty()
            || !value.method.bytes().all(|b| {
                b.is_ascii_lowercase() || b.is_ascii_digit() || b"!#$%&'*+-.^_`|~".contains(&b)
            })
            || !value.path.starts_with('/')
            || value.path.contains(['?', '#'])
            || value.path.chars().any(char::is_control)
            || value.route_contract_version.trim().is_empty()
        {
            return Err("action must use an explicit canonical method/path/contract version");
        }
        Ok(Self {
            method: value.method,
            path: value.path,
            route_contract_version: value.route_contract_version,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", try_from = "ProjectWire")]
pub enum ProjectContext {
    Account,
    Project { project_id: Id },
    Ungrouped,
    Unknown,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProjectWire {
    kind: String,
    project_id: Option<Id>,
}
impl TryFrom<ProjectWire> for ProjectContext {
    type Error = &'static str;
    fn try_from(value: ProjectWire) -> Result<Self, Self::Error> {
        match (value.kind.as_str(), value.project_id) {
            ("account", None) => Ok(Self::Account),
            ("project", Some(project_id)) => Ok(Self::Project { project_id }),
            ("ungrouped", None) => Ok(Self::Ungrouped),
            ("unknown", None) => Ok(Self::Unknown),
            _ => Err("invalid project kind/id combination"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PhysicalIdentity {
    pub kind: String,
    pub identity_namespace: String,
    pub immutable_uid: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "ResourceWire", deny_unknown_fields)]
pub struct ResourceRef {
    pub resource_type: String,
    pub resource_id: String,
    pub resource_arn: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ResourceWire {
    resource_type: String,
    resource_id: String,
    resource_arn: String,
}
fn valid_arn(value: &str, wildcard: bool) -> bool {
    let parts: Vec<_> = value.split(':').collect();
    if parts.len() != 5
        || parts[0] != "gjsl"
        || parts
            .iter()
            .any(|part| part.is_empty() || part.chars().any(char::is_whitespace))
    {
        return false;
    }
    let valid_segment = |segment: &str| {
        !segment.is_empty() && (!segment.contains('*') || (wildcard && segment == "*"))
    };
    if !parts[..4].iter().all(|part| valid_segment(part)) {
        return false;
    }
    if wildcard && parts[4] == "*" {
        return true;
    }
    match parts[4].split_once('/') {
        Some((kind, id)) => !id.contains('/') && valid_segment(kind) && valid_segment(id),
        None => false,
    }
}
impl TryFrom<ResourceWire> for ResourceRef {
    type Error = &'static str;
    fn try_from(value: ResourceWire) -> Result<Self, Self::Error> {
        if value.resource_type.is_empty()
            || value.resource_id.is_empty()
            || !valid_arn(&value.resource_arn, false)
        {
            return Err("resource reference requires a concrete five-segment ARN");
        }
        Ok(Self {
            resource_type: value.resource_type,
            resource_id: value.resource_id,
            resource_arn: value.resource_arn,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObjectStamp {
    pub provider: String,
    pub object_type: String,
    pub object_id: String,
    pub root_id: String,
    pub mapping_version: String,
    pub project: ProjectContext,
    pub resources: Vec<ResourceRef>,
    pub physical_identities: Vec<PhysicalIdentity>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NewRootTarget {
    pub provider: String,
    pub object_type: String,
    pub project: ProjectContext,
    pub mapping_version: String,
    pub resource_domain: String,
}

/// The discriminator and payload cannot disagree or carry both target kinds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "target_kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum AuthorizedTarget {
    Existing {
        target_key: String,
        target_role: String,
        action: RequestAction,
        stamp: ObjectStamp,
    },
    NewRoot {
        target_key: String,
        target_role: String,
        action: RequestAction,
        new_root: NewRootTarget,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", try_from = "ScopeWire")]
pub enum SelectedScope {
    AllAuthorized,
    Project { project_id: Id },
    Ungrouped,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", try_from = "ScopeWire")]
pub enum ClauseScope {
    All,
    Project { project_id: Id },
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ScopeWire {
    kind: String,
    project_id: Option<Id>,
}
impl TryFrom<ScopeWire> for SelectedScope {
    type Error = &'static str;
    fn try_from(value: ScopeWire) -> Result<Self, Self::Error> {
        match (value.kind.as_str(), value.project_id) {
            ("all_authorized", None) => Ok(Self::AllAuthorized),
            ("project", Some(project_id)) => Ok(Self::Project { project_id }),
            ("ungrouped", None) => Ok(Self::Ungrouped),
            _ => Err("invalid selected scope kind/id combination"),
        }
    }
}
impl TryFrom<ScopeWire> for ClauseScope {
    type Error = &'static str;
    fn try_from(value: ScopeWire) -> Result<Self, Self::Error> {
        match (value.kind.as_str(), value.project_id) {
            ("all", None) => Ok(Self::All),
            ("project", Some(project_id)) => Ok(Self::Project { project_id }),
            _ => Err("invalid clause scope kind/id combination"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", try_from = "PatternWire")]
pub enum ResourcePattern {
    Any,
    Exact { resource_arn: String },
    WholeSegmentStar { resource_arn: String },
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PatternWire {
    kind: String,
    resource_arn: Option<String>,
}
impl TryFrom<PatternWire> for ResourcePattern {
    type Error = &'static str;
    fn try_from(value: PatternWire) -> Result<Self, Self::Error> {
        match (value.kind.as_str(), value.resource_arn) {
            ("any", None) => Ok(Self::Any),
            ("exact", Some(resource_arn)) if valid_arn(&resource_arn, false) => {
                Ok(Self::Exact { resource_arn })
            }
            ("whole_segment_star", Some(resource_arn))
                if valid_arn(&resource_arn, true) && resource_arn.contains('*') =>
            {
                Ok(Self::WholeSegmentStar { resource_arn })
            }
            _ => Err("unsupported resource pattern grammar"),
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Clause {
    pub scope: ClauseScope,
    pub resource_patterns: Vec<ResourcePattern>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QueryConstraint {
    pub domain: String,
    pub selected_scope: SelectedScope,
    pub public_bypass: bool,
    pub allow_clauses: Vec<Clause>,
    pub deny_clauses: Vec<Clause>,
    pub version: AuthVersion,
    pub lease: SourceLease,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionContext {
    pub protocol: ProtocolV2,
    pub request_id: UuidText,
    pub audience: String,
    pub subject: RequestSubject,
    pub action: RequestAction,
    pub request_digest: String,
    pub targets_digest: String,
    pub targets: Vec<AuthorizedTarget>,
    pub query_constraint: Option<QueryConstraint>,
    pub version: AuthVersion,
    pub lease: SourceLease,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VisibilityState {
    Ready,
    #[default]
    Unavailable,
}

#[cfg(test)]
pub(crate) mod project_protocol_tests {
    use super::*;
    use serde_json::{json, Value};

    pub(crate) fn context_json() -> Value {
        json!({
            "protocol":2,"request_id":"11111111-1111-4111-8111-111111111111","audience":"deployment",
            "subject":{"tenant_id":"4","user_id":"9007199254740993","tenant_type":"team"},
            "action":{"method":"post","path":"/api/task/update","route_contract_version":"project-v2"},
            "request_digest":"a".repeat(64),"targets_digest":"b".repeat(64),"targets":[{
                "target_kind":"existing","target_key":"c".repeat(64),"target_role":"primary",
                "action":{"method":"post","path":"/api/task/update","route_contract_version":"project-v2"},
                "stamp":{"provider":"deployment","object_type":"task","object_id":"123","root_id":"123","mapping_version":"project-v2",
                    "project":{"kind":"project","project_id":"16"},
                    "resources":[{"resource_type":"task","resource_id":"456","resource_arn":"gjsl:deployment:bj:4:task/456"}],"physical_identities":[]}
            }],"query_constraint":null,
            "version":{"cache_protocol":2,"auth_epoch":"22222222-2222-4222-8222-222222222222","tree_token":"0","tenant_token":"9223372036854775807"},
            "lease":{"source_started_at_unix_ms":1000000,"source_expires_at_unix_ms":1000001}
        })
    }

    #[test]
    fn ids_and_tokens_roundtrip_without_number_precision_loss() {
        let id: Id = serde_json::from_value(json!("9223372036854775807")).unwrap();
        assert_eq!(id.get(), i64::MAX);
        assert_eq!(
            serde_json::to_value(id).unwrap(),
            json!("9223372036854775807")
        );
        let zero: Token = serde_json::from_value(json!("0")).unwrap();
        assert_eq!(zero.get(), 0);
        for invalid in [
            json!(0),
            json!(1),
            json!(-1),
            json!(1.0),
            json!("0"),
            json!("-1"),
            json!("+1"),
            json!("01"),
            json!("1.0"),
            json!("1e3"),
            json!(""),
            json!("9223372036854775808"),
            json!(" 1"),
            json!("１"),
        ] {
            assert!(serde_json::from_value::<Id>(invalid).is_err());
        }
        for invalid in [
            json!(0),
            json!(-1),
            json!("-1"),
            json!("00"),
            json!("9223372036854775808"),
        ] {
            assert!(serde_json::from_value::<Token>(invalid).is_err());
        }
    }

    #[test]
    fn project_kind_and_id_must_agree() {
        for valid in [
            json!({"kind":"account"}),
            json!({"kind":"ungrouped"}),
            json!({"kind":"unknown"}),
            json!({"kind":"project","project_id":"3"}),
        ] {
            assert!(serde_json::from_value::<ProjectContext>(valid).is_ok());
        }
        for invalid in [
            json!({"kind":"project"}),
            json!({"kind":"project","project_id":null}),
            json!({"kind":"account","project_id":"3"}),
            json!({"kind":"ungrouped","project_id":"3"}),
            json!({"kind":"unknown","project_id":"3"}),
            json!({"kind":"all"}),
            json!({"kind":"project","project_id":3}),
            json!({"kind":"account","extra":true}),
        ] {
            assert!(serde_json::from_value::<ProjectContext>(invalid).is_err());
        }
    }

    #[test]
    fn complete_context_roundtrips_and_rejects_missing_or_unknown_security_fields() {
        let original = context_json();
        let parsed: ExecutionContext = serde_json::from_value(original.clone()).unwrap();
        assert_eq!(serde_json::to_value(parsed).unwrap(), original);
        for field in [
            "protocol",
            "request_id",
            "audience",
            "subject",
            "action",
            "version",
            "lease",
            "targets",
        ] {
            let mut value = context_json();
            value.as_object_mut().unwrap().remove(field);
            assert!(
                serde_json::from_value::<ExecutionContext>(value).is_err(),
                "missing {field}"
            );
        }
        for pointer in ["/protocol", "/version/cache_protocol"] {
            let mut value = context_json();
            *value.pointer_mut(pointer).unwrap() = json!(3);
            assert!(serde_json::from_value::<ExecutionContext>(value).is_err());
        }
        let mut value = context_json();
        value["scope_all"] = json!(true);
        assert!(serde_json::from_value::<ExecutionContext>(value).is_err());
        let mut value = context_json();
        value["targets"][0]["target_kind"] = json!("bypass");
        assert!(serde_json::from_value::<ExecutionContext>(value).is_err());
        let mut value = context_json();
        value["targets"][0]["new_root"] = json!({});
        assert!(serde_json::from_value::<ExecutionContext>(value).is_err());
        for pointer in ["/request_id", "/version/auth_epoch"] {
            let mut value = context_json();
            *value.pointer_mut(pointer).unwrap() = json!("not-a-uuid");
            assert!(serde_json::from_value::<ExecutionContext>(value).is_err());
        }
    }

    #[test]
    fn query_constraints_have_no_allow_or_public_defaults() {
        let value = json!({"domain":"deployment-project-v2","selected_scope":{"kind":"all_authorized"},"public_bypass":false,
            "allow_clauses":[],"deny_clauses":[{"scope":{"kind":"project","project_id":"3"},"resource_patterns":[{"kind":"whole_segment_star","resource_arn":"gjsl:deployment:*:4:task/*"}]}],
            "version":context_json()["version"],"lease":context_json()["lease"]});
        let parsed: QueryConstraint = serde_json::from_value(value.clone()).unwrap();
        assert!(!parsed.public_bypass);
        assert!(parsed.allow_clauses.is_empty());
        for field in [
            "public_bypass",
            "allow_clauses",
            "deny_clauses",
            "selected_scope",
            "version",
            "lease",
        ] {
            let mut missing = value.clone();
            missing.as_object_mut().unwrap().remove(field);
            assert!(serde_json::from_value::<QueryConstraint>(missing).is_err());
        }
        for invalid in [
            json!({"kind":"bogus"}),
            json!({"kind":"project"}),
            json!({"kind":"project","project_id":0}),
        ] {
            assert!(serde_json::from_value::<SelectedScope>(invalid).is_err());
        }
        for invalid in [
            json!({"kind":"bogus"}),
            json!({"kind":"exact","resource_arn":"*"}),
            json!({"kind":"whole_segment_star","resource_arn":"gjsl:deployment:bj*:4:task/*"}),
            json!({"kind":"whole_segment_star","resource_arn":"gjsl:deployment:bj:4:task/1"}),
        ] {
            assert!(serde_json::from_value::<ResourcePattern>(invalid).is_err());
        }
    }

    #[test]
    fn unit_variants_reject_extra_or_conflicting_payloads() {
        for value in [
            json!({"kind":"all_authorized","project_id":"3"}),
            json!({"kind":"all_authorized","extra":true}),
            json!({"kind":"ungrouped","project_id":"3"}),
        ] {
            assert!(serde_json::from_value::<SelectedScope>(value).is_err());
        }
        for value in [
            json!({"kind":"all","project_id":"3"}),
            json!({"kind":"all","extra":true}),
        ] {
            assert!(serde_json::from_value::<ClauseScope>(value).is_err());
        }
        for value in [
            json!({"kind":"any","resource_arn":"gjsl:x:y:4:task/1"}),
            json!({"kind":"any","extra":true}),
        ] {
            assert!(serde_json::from_value::<ResourcePattern>(value).is_err());
        }
    }

    #[test]
    fn concrete_resources_and_source_time_shape_are_required() {
        for arn in [
            "*",
            "gjsl:deployment:*:4:task/1",
            "gjsl:deployment::4:task/1",
            "gjsl:deployment:bj:4:",
            "gjsl:deployment:bj:4:task/",
        ] {
            assert!(serde_json::from_value::<ResourceRef>(
                json!({"resource_type":"task","resource_id":"1","resource_arn":arn})
            )
            .is_err());
        }
        for (start, end) in [(-1_i64, 1_i64), (2, 1), (0, 9_007_199_254_740_992)] {
            assert!(serde_json::from_value::<SourceLease>(
                json!({"source_started_at_unix_ms":start,"source_expires_at_unix_ms":end})
            )
            .is_err());
        }
        let mut value = context_json();
        value["action"]["method"] = json!("POST");
        assert!(serde_json::from_value::<ExecutionContext>(value).is_err());
    }
}
