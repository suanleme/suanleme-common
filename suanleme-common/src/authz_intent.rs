//! Independent authorization-intent protocol and optional request wrapper.
//!
//! These types remain available to consumers of the intent protocol. The current V4 gateway
//! instead forwards [`ResourceGroupAccess`] in [`CommonRequest`]. An intent is neither an
//! authorization result nor a capability; deserialization alone does not authenticate its source
//! or permit the operation.

use serde::ser::SerializeMap;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::authz::{ProjectContext, RequestSubject, UuidText};
use crate::business::{CommonRequest, Merchant, ResourceGroupAccess, Tenant};

/// Wire protocol discriminator for [`AuthorizationIntent`].
pub const AUTHORIZATION_INTENT_PROTOCOL: u16 = 1;

/// Trusted-channel input for service-local authorization resolution.
///
/// Route/action and product identity deliberately stay out of this common envelope. The receiving
/// service derives them from its registered handler and resolves product resources locally.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthorizationIntent {
    pub protocol: u16,
    pub request_id: UuidText,
    pub subject: RequestSubject,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requested_project: Option<ProjectContext>,
}

impl AuthorizationIntent {
    /// Validate the supported protocol and the only project hint accepted by business services.
    ///
    /// Missing project means that the request did not select a project. When present, the value
    /// must name one concrete project; account/ungrouped/unknown are decision outputs, not valid
    /// client selection hints.
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.protocol != AUTHORIZATION_INTENT_PROTOCOL {
            return Err("unsupported authorization intent protocol");
        }
        if self.subject.tenant_type.trim().is_empty() {
            return Err("authorization intent subject tenant_type must be non-empty");
        }
        if !matches!(
            &self.requested_project,
            None | Some(ProjectContext::Project { .. })
        ) {
            return Err("authorization intent requested_project must be absent or a project");
        }
        Ok(())
    }
}

/// Backward-compatible POST envelope for routes that support service-local authorization.
///
/// Its flattened wire shape shares the minimal [`CommonRequest`] and adds only the optional
/// intent. Removed envelope fields are ignored on input and are not retained or forwarded.
#[derive(Debug, Clone)]
pub struct AuthorizationRequest<T> {
    pub envelope: CommonRequest<T>,
    pub authorization_intent: Option<AuthorizationIntent>,
}

#[derive(Deserialize)]
struct AuthorizationRequestWire<T> {
    version: Option<String>,
    user_id: Option<i64>,
    merchant: Option<Merchant>,
    tenant: Option<Tenant>,
    token: Option<String>,
    sign_str: Option<String>,
    timestamp: u128,
    data: Option<T>,
    #[serde(default)]
    resource_group_access: Option<ResourceGroupAccess>,
    #[serde(default)]
    authorization_intent: Option<AuthorizationIntent>,
}

impl<T: Serialize> Serialize for AuthorizationRequest<T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let request = &self.envelope;
        // Keep this wrapper synchronized with CommonRequest without changing CommonRequest itself.
        let CommonRequest {
            version: _,
            user_id: _,
            merchant: _,
            tenant: _,
            token: _,
            sign_str: _,
            timestamp: _,
            data: _,
            resource_group_access: _,
        } = request;
        let optional_fields = usize::from(request.resource_group_access.is_some())
            + usize::from(self.authorization_intent.is_some());
        let mut map = serializer.serialize_map(Some(8 + optional_fields))?;
        map.serialize_entry("version", &request.version)?;
        map.serialize_entry("user_id", &request.user_id)?;
        map.serialize_entry("merchant", &request.merchant)?;
        map.serialize_entry("tenant", &request.tenant)?;
        map.serialize_entry("token", &request.token)?;
        map.serialize_entry("sign_str", &request.sign_str)?;
        map.serialize_entry("timestamp", &request.timestamp)?;
        map.serialize_entry("data", &request.data)?;
        if let Some(resource_group_access) = &request.resource_group_access {
            map.serialize_entry("resource_group_access", resource_group_access)?;
        }
        if let Some(intent) = &self.authorization_intent {
            map.serialize_entry("authorization_intent", intent)?;
        }
        map.end()
    }
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for AuthorizationRequest<T> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = AuthorizationRequestWire::<T>::deserialize(deserializer)?;
        Ok(Self {
            envelope: CommonRequest {
                version: wire.version,
                user_id: wire.user_id,
                merchant: wire.merchant,
                tenant: wire.tenant,
                token: wire.token,
                sign_str: wire.sign_str,
                timestamp: wire.timestamp,
                data: wire.data,
                resource_group_access: wire.resource_group_access,
            },
            authorization_intent: wire.authorization_intent,
        })
    }
}

impl<T> AuthorizationRequest<T> {
    pub fn into_parts(self) -> (CommonRequest<T>, Option<AuthorizationIntent>) {
        (self.envelope, self.authorization_intent)
    }

    /// Transform only business data while preserving both the established envelope and intent.
    pub fn map_data<U>(self, map: impl FnOnce(T) -> U) -> AuthorizationRequest<U> {
        AuthorizationRequest {
            envelope: self.envelope.map_data(map),
            authorization_intent: self.authorization_intent,
        }
    }
}

impl<T> From<CommonRequest<T>> for AuthorizationRequest<T> {
    fn from(envelope: CommonRequest<T>) -> Self {
        Self {
            envelope,
            authorization_intent: None,
        }
    }
}

impl<T> Default for AuthorizationRequest<T> {
    fn default() -> Self {
        Self::from(CommonRequest::default())
    }
}

impl<T> std::ops::Deref for AuthorizationRequest<T> {
    type Target = CommonRequest<T>;

    fn deref(&self) -> &Self::Target {
        &self.envelope
    }
}

impl<T> std::ops::DerefMut for AuthorizationRequest<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.envelope
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::authz::Id;
    use serde_json::json;

    fn intent(requested_project: Option<ProjectContext>) -> AuthorizationIntent {
        AuthorizationIntent {
            protocol: AUTHORIZATION_INTENT_PROTOCOL,
            request_id: UuidText::new("11111111-1111-4111-8111-111111111111").unwrap(),
            subject: RequestSubject {
                tenant_id: Id::new(4).unwrap(),
                user_id: Id::new(7).unwrap(),
                tenant_type: "team".to_owned(),
            },
            requested_project,
        }
    }

    #[test]
    fn absent_or_concrete_project_is_valid() {
        assert!(intent(None).validate().is_ok());
        assert!(intent(Some(ProjectContext::Project {
            project_id: Id::new(16).unwrap(),
        }))
        .validate()
        .is_ok());
    }

    #[test]
    fn unsupported_protocol_and_empty_tenant_type_are_rejected() {
        let mut value = intent(None);
        value.protocol = 2;
        assert_eq!(
            value.validate(),
            Err("unsupported authorization intent protocol")
        );

        value.protocol = AUTHORIZATION_INTENT_PROTOCOL;
        value.subject.tenant_type = " \t".to_owned();
        assert_eq!(
            value.validate(),
            Err("authorization intent subject tenant_type must be non-empty")
        );
    }

    #[test]
    fn non_project_contexts_are_rejected() {
        for requested_project in [
            ProjectContext::Account,
            ProjectContext::Ungrouped,
            ProjectContext::Unknown,
        ] {
            assert_eq!(
                intent(Some(requested_project)).validate(),
                Err("authorization intent requested_project must be absent or a project")
            );
        }
    }

    #[test]
    fn serde_round_trip_is_exact_and_rejects_unknown_fields() {
        let value = json!({
            "protocol": 1,
            "request_id": "11111111-1111-4111-8111-111111111111",
            "subject": {"tenant_id":"4","user_id":"7","tenant_type":"team"},
            "requested_project": {"kind":"project","project_id":"16"}
        });
        let decoded: AuthorizationIntent = serde_json::from_value(value.clone()).unwrap();
        assert!(decoded.validate().is_ok());
        assert_eq!(serde_json::to_value(decoded).unwrap(), value);

        let mut unknown = value;
        unknown["service"] = json!("product-a");
        assert!(serde_json::from_value::<AuthorizationIntent>(unknown).is_err());
    }

    #[test]
    fn post_wrapper_keeps_legacy_common_request_wire_shape() {
        let legacy = json!({
            "version":"1","user_id":7,"merchant":null,"tenant":null,
            "token":null,"sign_str":null,"timestamp":1,"data":{"task_id":9}
        });
        let decoded: AuthorizationRequest<serde_json::Value> =
            serde_json::from_value(legacy.clone()).unwrap();
        assert!(decoded.authorization_intent.is_none());
        assert_eq!(serde_json::to_value(decoded).unwrap(), legacy);
    }

    #[test]
    fn post_wrapper_round_trips_rga_and_legacy_tenant() {
        let value = json!({
            "version":"1","user_id":7,"merchant":null,
            "tenant": {
                "tenant_id":4,"tenant_type":"team","tenant_name":"Team",
                "permission":"legacy","is_admin":false
            },
            "token":null,"sign_str":null,"timestamp":1,"data":{"task_id":9},
            "resource_group_access": {
                "resource_group_ids":["16"],"resource_group_has_ids":["16","17"]
            },
            "authorization_intent": intent(None)
        });
        let decoded: AuthorizationRequest<serde_json::Value> =
            serde_json::from_value(value.clone()).unwrap();
        assert_eq!(serde_json::to_value(&decoded).unwrap(), value);
        let (envelope, intent) = decoded.into_parts();
        assert!(intent.unwrap().validate().is_ok());
        assert_eq!(envelope.tenant.unwrap().tenant_id, 4);
        assert_eq!(
            envelope.resource_group_access.unwrap().effective_ids(),
            vec![Id::new(16).unwrap()]
        );
    }

    #[test]
    fn post_wrapper_drops_removed_metadata_without_changing_business_data() {
        let expected = json!({
            "version":"1","user_id":7,"merchant":null,
            "tenant": {
                "tenant_id":4,"tenant_type":"team","tenant_name":"Team",
                "permission":"legacy","is_admin":false
            },
            "token":null,"sign_str":null,"timestamp":1,
            "data":{"auth_context":{"business":"keep"},"resource_group_id":9},
            "resource_group_access":{"resource_group_has_ids":[]},
            "authorization_intent":intent(None)
        });
        let mut obsolete = expected.clone();
        // These are no longer protocol fields, even when their old shapes are invalid.
        obsolete["resource_group_id"] = json!({"obsolete":true});
        obsolete["auth_context"] = json!({"obsolete":true});
        obsolete["transition_auth_context"] = json!({"obsolete":true});
        for field in [
            "visible_project_ids",
            "scope_all",
            "visibility_state",
            "visibility_version",
            "visibility_lease",
        ] {
            obsolete["tenant"][field] = json!({"obsolete":true});
        }
        let decoded: AuthorizationRequest<serde_json::Value> =
            serde_json::from_value(obsolete).unwrap();
        assert!(decoded.resource_group_access.is_some());
        assert_eq!(serde_json::to_value(decoded).unwrap(), expected);
    }

    #[test]
    fn post_wrapper_round_trips_intent_without_changing_common_request() {
        let value = json!({
            "version":"1","user_id":7,"merchant":null,"tenant":null,
            "token":null,"sign_str":null,"timestamp":1,"data":{"task_id":9},
            "authorization_intent": {
                "protocol": 1,
                "request_id": "11111111-1111-4111-8111-111111111111",
                "subject": {"tenant_id":"4","user_id":"7","tenant_type":"team"},
                "requested_project": {"kind":"project","project_id":"16"}
            }
        });
        let decoded: AuthorizationRequest<serde_json::Value> =
            serde_json::from_value(value.clone()).unwrap();
        assert!(decoded
            .authorization_intent
            .as_ref()
            .unwrap()
            .validate()
            .is_ok());
        assert_eq!(serde_json::to_value(decoded).unwrap(), value);

        let mut invalid = value;
        invalid["authorization_intent"]["service"] = json!("product-a");
        assert!(
            serde_json::from_value::<AuthorizationRequest<serde_json::Value>>(invalid).is_err()
        );
    }

    #[test]
    fn map_data_preserves_intent_and_envelope_fields() {
        let mut request = AuthorizationRequest::<String>::default();
        request.envelope.timestamp = 7;
        request.envelope.data = Some("9".to_owned());
        request.envelope.tenant = Some(Tenant {
            tenant_id: 4,
            tenant_type: "team".to_owned(),
            tenant_name: "Team".to_owned(),
            permission: "legacy".to_owned(),
            is_admin: false,
        });
        let access = ResourceGroupAccess {
            resource_group_ids: Some(vec![Id::new(16).unwrap()]),
            resource_group_has_ids: vec![Id::new(16).unwrap()],
            ungrouped_resource_allowed: false,
        };
        request.envelope.resource_group_access = Some(access.clone());
        request.authorization_intent = Some(intent(None));
        let mapped = request.map_data(|value| value.parse::<i64>().unwrap());
        assert_eq!(mapped.envelope.timestamp, 7);
        assert_eq!(mapped.envelope.data, Some(9));
        assert_eq!(mapped.envelope.tenant.unwrap().tenant_name, "Team");
        assert_eq!(mapped.envelope.resource_group_access, Some(access));
        assert!(mapped.authorization_intent.is_some());
    }
}
