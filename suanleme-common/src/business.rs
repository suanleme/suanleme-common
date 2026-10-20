use suanleme_macro::Data;

use crate::utils::date_util::get_now_date_time_as_millis;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Data)]
pub struct CommonRequest<T> {
    pub version: Option<String>,
    pub user_id: Option<i64>,
    pub merchant: Option<Merchant>,
    pub tenant: Option<Tenant>,
    pub token: Option<String>,
    pub sign_str: Option<String>,
    pub timestamp: u128,
    pub data: Option<T>,
    /// Single-project compatibility projection; never a substitute for V2 context.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource_group_id: Option<crate::authz::Id>,
    /// Trusted-channel transport only; deserialization does not authenticate it.
    /// V2 execution context (`protocol = 2`); never used for the C1 transition package.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth_context: Option<crate::authz::ExecutionContext>,
    /// **Explicit C1 transition execution context** (`protocol = 1`, schema 1).
    ///
    /// Transport for `resources/authz_transition`: a gateway-injected trusted-channel
    /// field carrying the existing C1 token pair and the C2 absolute source deadline.
    /// It is intentionally **not** an alias of [`Self::auth_context`]: the two channels
    /// never interpret each other's value, and neither may be upgraded in place while
    /// the V2 epoch/persistent-token rollout does not exist. Delete this field (and its
    /// uses in the gateway/business guards) when V2 replaces the transition protocol.
    ///
    /// `serde(default)` keeps old requests without the key decodable; serialization adds
    /// the key only when it is present. Deserialization does not authenticate its source.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transition_auth_context: Option<crate::authz_transition::TransitionExecutionContext>,
}

impl<T> CommonRequest<T> {
    /// Transform only business data, moving every trusted envelope field intact.
    /// The exhaustive destructure intentionally makes future fields a compile error.
    pub fn map_data<U>(self, map: impl FnOnce(T) -> U) -> CommonRequest<U> {
        let CommonRequest {
            version,
            user_id,
            merchant,
            tenant,
            token,
            sign_str,
            timestamp,
            data,
            resource_group_id,
            auth_context,
            transition_auth_context,
        } = self;
        CommonRequest {
            version,
            user_id,
            merchant,
            tenant,
            token,
            sign_str,
            timestamp,
            data: data.map(map),
            resource_group_id,
            auth_context,
            transition_auth_context,
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Data)]
pub struct CommonResponse<T> {
    pub code: String,
    pub message: Option<String>,
    pub data: Option<T>,
}

impl<T> CommonResponse<T> {
    pub fn into_data(self) -> Option<T> {
        let CommonResponse {
            code: _,
            message: _,
            data,
        } = self;
        data
    }
}

#[derive(Default, Clone, serde::Serialize, serde::Deserialize, Debug)]
pub struct Tenant {
    pub tenant_id: i64,
    pub tenant_type: String,
    pub tenant_name: String,
    pub permission: String,
    pub is_admin: bool,
    /// Display projection only; never use this union as an action authorization.
    #[serde(default)]
    pub visible_project_ids: Vec<crate::authz::Id>,
    #[serde(default)]
    pub scope_all: bool,
    #[serde(default)]
    pub visibility_state: crate::authz::VisibilityState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub visibility_version: Option<crate::authz::AuthVersion>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub visibility_lease: Option<crate::authz::SourceLease>,
}

#[derive(Default, Clone, serde::Serialize, serde::Deserialize, Debug)]
pub struct Merchant {
    pub id: i64,
    pub token: String,
    pub merchant_mark: String,
}

impl<T: serde::Serialize> From<CommonResponse<T>> for serde_json::Value {
    fn from(value: CommonResponse<T>) -> Self {
        serde_json::to_value(&value).unwrap()
    }
}

#[derive(Debug, Default, Clone, serde::Serialize, serde::Deserialize, Data)]
pub struct Nil;

impl<T> Default for CommonRequest<T> {
    fn default() -> Self {
        Self {
            version: Default::default(),
            user_id: Default::default(),
            merchant: Default::default(),
            tenant: Default::default(),
            token: Default::default(),
            sign_str: Default::default(),
            timestamp: get_now_date_time_as_millis(),
            data: None,
            resource_group_id: None,
            auth_context: None,
            transition_auth_context: None,
        }
    }
}

impl<T> Default for CommonResponse<T> {
    fn default() -> Self {
        Self {
            code: Default::default(),
            message: Default::default(),
            data: None,
        }
    }
}

#[cfg(test)]
mod project_protocol_tests {
    use super::*;
    use crate::authz::{project_protocol_tests::context_json, VisibilityState};
    use serde_json::json;

    fn transition_auth_state_json() -> serde_json::Value {
        json!({
            "schema": 1,
            "tree_token": 12,
            "tenant_token": 34,
            "source_started_at_unix_ms": 1_800_000_000_000_i64,
            "source_expires_at_unix_ms": 1_800_000_060_000_i64
        })
    }

    fn transition_context_json() -> serde_json::Value {
        json!({
            "protocol": 1,
            "request_id": "11111111-1111-4111-8111-111111111111",
            "audience": "gj-deployment-api",
            "subject": {"tenant_id":"4","user_id":"7","tenant_type":"team"},
            "action": {"method":"post","path":"/api/deployment/task/create",
                "route_contract_version":"deployment-project-transition-1"},
            "request_digest": "b".repeat(64),
            "targets_digest": "c".repeat(64),
            "targets": [],
            "query_constraint": null,
            "auth_state": transition_auth_state_json()
        })
    }

    #[test]
    fn legacy_request_and_tenant_default_to_unavailable_without_changing_numeric_ids() {
        let request: CommonRequest<String> = serde_json::from_value(json!({
            "version":"1", "user_id":7, "merchant":{"id":1,"token":"opaque","merchant_mark":"m"},
            "tenant":{"tenant_id":4,"tenant_type":"team","tenant_name":"old","permission":"{}","is_admin":false},
            "token":"t","sign_str":"s","timestamp":1,"data":"payload"
        })).unwrap();
        assert!(request.resource_group_id.is_none());
        assert!(request.auth_context.is_none());
        assert_eq!(request.user_id, Some(7));
        let tenant = request.tenant.unwrap();
        assert_eq!(tenant.tenant_id, 4);
        assert_eq!(tenant.visibility_state, VisibilityState::Unavailable);
        assert!(!tenant.scope_all);
        assert!(tenant.visible_project_ids.is_empty());
        assert!(tenant.visibility_version.is_none() && tenant.visibility_lease.is_none());
        let default = CommonRequest::<String>::default();
        assert!(default.resource_group_id.is_none() && default.auth_context.is_none());
        assert!(default.transition_auth_context.is_none());
        assert_eq!(
            Tenant::default().visibility_state,
            VisibilityState::Unavailable
        );
        assert!(serde_json::from_value::<Tenant>(json!({"tenant_id":4,"tenant_type":"team","tenant_name":"old","permission":"{}","is_admin":false,"visibility_state":"allow"})).is_err());
    }

    #[test]
    fn map_data_moves_every_trusted_field_without_a_clone_bound() {
        #[derive(serde::Deserialize)]
        struct MoveOnly(String);
        let value = json!({
            "version":"2","user_id":9007199254740993_i64,
            "merchant":{"id":8,"token":"merchant-token","merchant_mark":"merchant"},
            "tenant":{"tenant_id":4,"tenant_type":"team","tenant_name":"example","permission":"legacy",
                "is_admin":false,"visible_project_ids":["16","9223372036854775807"],"scope_all":false,
                "visibility_state":"ready","visibility_version":context_json()["version"],"visibility_lease":context_json()["lease"]},
            "token":"request-token","sign_str":"signature","timestamp":123456789,
            "data":"move-only","resource_group_id":"16","auth_context":context_json(),
            "transition_auth_context":transition_context_json()
        });
        let request: CommonRequest<MoveOnly> = serde_json::from_value(value.clone()).unwrap();
        let mapped = request.map_data(|MoveOnly(data)| data.len());
        let mut expected = value;
        expected["data"] = json!(9);
        assert_eq!(serde_json::to_value(mapped).unwrap(), expected);
    }

    #[test]
    fn map_data_does_not_invoke_converter_for_missing_data() {
        let request = CommonRequest::<String>::default();
        let before = request.timestamp;
        let result: CommonRequest<usize> =
            request.map_data(|_| panic!("None must not call converter"));
        assert!(result.data.is_none());
        assert_eq!(result.timestamp, before);
        assert!(result.resource_group_id.is_none() && result.auth_context.is_none());
        assert!(result.transition_auth_context.is_none());
    }

    #[test]
    fn legacy_requests_stay_decodable_and_serialize_without_the_transition_key() {
        let legacy = json!({
            "version":"1","user_id":7,
            "merchant":{"id":1,"token":"opaque","merchant_mark":"m"},
            "tenant":{"tenant_id":4,"tenant_type":"team","tenant_name":"old",
                "permission":"{}","is_admin":false},
            "token":"t","sign_str":"s","timestamp":1,"data":"payload"
        });
        let request: CommonRequest<String> = serde_json::from_value(legacy.clone()).unwrap();
        assert!(request.transition_auth_context.is_none());
        let serialized = serde_json::to_value(&request).unwrap();
        assert!(
            serialized.get("transition_auth_context").is_none(),
            "None must not add the key"
        );
        // Existing fields keep their original line format (display-only Tenant fields
        // added by the same local patch always serialize; that is not this channel).
        assert_eq!(serialized["data"], json!("payload"));
        assert_eq!(serialized["user_id"], json!(7));
        assert_eq!(serialized["tenant"]["tenant_id"], json!(4));
    }

    #[test]
    fn transition_context_is_a_separate_typed_channel_from_v2_auth_context() {
        // Only the transition channel set: round-trips through the dedicated field.
        let mut value = json!({
            "version":"1","user_id":7,"timestamp":1,"data":"payload",
            "resource_group_id":"16",
            "transition_auth_context": transition_context_json()
        });
        value["tenant"] = json!({"tenant_id":4,"tenant_type":"team","tenant_name":"t",
            "permission":"{}","is_admin":false});
        let request: CommonRequest<String> = serde_json::from_value(value.clone()).unwrap();
        assert!(request.auth_context.is_none());
        let context = request.transition_auth_context.clone().unwrap();
        assert_eq!(context.protocol, 1);
        assert_eq!(context.action.method, "post");
        let serialized = serde_json::to_value(&request).unwrap();
        assert_eq!(
            serialized["transition_auth_context"], value["transition_auth_context"],
            "transition channel round-trips byte-for-byte"
        );

        // Both channels may be present and each keeps its own meaning.
        value["auth_context"] = context_json();
        let request: CommonRequest<String> = serde_json::from_value(value.clone()).unwrap();
        assert!(request.auth_context.is_some());
        assert!(request.transition_auth_context.is_some());
        let serialized = serde_json::to_value(&request).unwrap();
        assert_eq!(
            serialized["auth_context"], value["auth_context"],
            "V2 channel keeps its own value"
        );
        assert_eq!(
            serialized["transition_auth_context"], value["transition_auth_context"],
            "transition channel keeps its own value"
        );

        // The channels never interpret each other: a transition document in the V2
        // field is rejected, and a V2 document in the transition field is rejected.
        let mut wrong_v2 = value.clone();
        wrong_v2["auth_context"] = transition_context_json();
        assert!(serde_json::from_value::<CommonRequest<String>>(wrong_v2).is_err());
        let mut wrong_transition = value;
        wrong_transition["transition_auth_context"] = context_json();
        assert!(serde_json::from_value::<CommonRequest<String>>(wrong_transition).is_err());

        // Unknown fields inside the transition envelope still fail closed.
        let mut unknown = transition_context_json();
        unknown["epoch"] = json!("22222222-2222-4222-8222-222222222222");
        let mut request_value = json!({"timestamp":1,"transition_auth_context": unknown});
        request_value["tenant"] = json!({"tenant_id":4,"tenant_type":"team","tenant_name":"t",
            "permission":"{}","is_admin":false});
        assert!(serde_json::from_value::<CommonRequest<String>>(request_value).is_err());
    }
}
