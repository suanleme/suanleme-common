use suanleme_macro::Data;

use crate::utils::date_util::get_now_date_time_as_millis;

/// Project scope selected by the request and allowed by IAM for this exact action.
///
/// This is trusted envelope metadata injected by the gateway. It deliberately contains only
/// project IDs and ungrouped visibility: business data stays in [`CommonRequest::data`], while
/// policy documents, resource targets and IAM protocol details never enter a business DTO.
#[derive(Debug, Default, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceGroupAccess {
    /// Optional request-side narrowing. `None` means "all IDs in
    /// [`Self::resource_group_has_ids`]"; `Some([])` is an explicit empty selection.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource_group_ids: Option<Vec<crate::authz::Id>>,
    /// IDs IAM authorized for the current method/path. An empty vector means no project access.
    pub resource_group_has_ids: Vec<crate::authz::Id>,
    /// Whether IAM allows ungrouped resources for this action. Only an all-projects request can
    /// use this permission; a project selection always excludes ungrouped resources.
    #[serde(default, skip_serializing_if = "is_false")]
    pub ungrouped_resource_allowed: bool,
}

fn is_false(value: &bool) -> bool {
    !*value
}

impl ResourceGroupAccess {
    fn normalized_ids(mut ids: Vec<crate::authz::Id>) -> Vec<crate::authz::Id> {
        ids.sort_unstable();
        ids.dedup();
        ids
    }

    /// Return a canonical copy with both ID sets sorted and deduplicated.
    pub fn normalized(&self) -> Self {
        Self {
            resource_group_ids: self.resource_group_ids.clone().map(Self::normalized_ids),
            resource_group_has_ids: Self::normalized_ids(self.resource_group_has_ids.clone()),
            ungrouped_resource_allowed: self.ungrouped_resource_allowed,
        }
    }

    /// Ungrouped visibility applies only when the request selects all authorized projects.
    pub fn allows_ungrouped(&self) -> bool {
        self.resource_group_ids.is_none() && self.ungrouped_resource_allowed
    }

    /// Effective project IDs for the business operation.
    ///
    /// No request-side selection means every authorized ID. An explicit selection is intersected
    /// with the authorized set; callers never need to interpret IAM policy data themselves.
    pub fn effective_ids(&self) -> Vec<crate::authz::Id> {
        let normalized = self.normalized();
        let Some(requested) = normalized.resource_group_ids else {
            return normalized.resource_group_has_ids;
        };
        let authorized = normalized.resource_group_has_ids;
        requested
            .into_iter()
            .filter(|id| authorized.binary_search(id).is_ok())
            .collect()
    }
}

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
    /// Gateway-injected action-specific project scope. `None` keeps the legacy request path;
    /// `Some` always denotes IAM mode, including an empty authorized set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource_group_access: Option<ResourceGroupAccess>,
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
            resource_group_access,
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
            resource_group_access,
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
            resource_group_access: None,
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
    use crate::authz::Id;
    use serde_json::json;

    #[test]
    fn legacy_request_and_tenant_keep_function_test_wire_shape() {
        // The complete field shape of the function-test dependency (common 0.1.67).
        let legacy = json!({
            "version":"1", "user_id":9007199254740993_i64,
            "merchant":{"id":1,"token":"opaque","merchant_mark":"m"},
            "tenant":{"tenant_id":4,"tenant_type":"team","tenant_name":"old",
                "permission":"{}","is_admin":false},
            "token":"t","sign_str":"s","timestamp":1,"data":"payload"
        });
        let request: CommonRequest<String> = serde_json::from_value(legacy.clone()).unwrap();
        assert!(request.resource_group_access.is_none());
        assert_eq!(serde_json::to_value(request).unwrap(), legacy);
        assert!(CommonRequest::<String>::default()
            .resource_group_access
            .is_none());
        assert_eq!(
            serde_json::to_value(Tenant::default()).unwrap(),
            json!({"tenant_id":0,"tenant_type":"","tenant_name":"","permission":"","is_admin":false})
        );
        assert!(
            serde_json::from_value::<CommonRequest<String>>(json!({"data":"payload"})).is_err()
        );
    }

    #[test]
    fn map_data_moves_every_trusted_field_without_a_clone_bound() {
        #[derive(serde::Deserialize)]
        struct MoveOnly(String);
        let value = json!({
            "version":"2","user_id":9007199254740993_i64,
            "merchant":{"id":8,"token":"merchant-token","merchant_mark":"merchant"},
            "tenant":{"tenant_id":4,"tenant_type":"team","tenant_name":"example","permission":"legacy",
                "is_admin":false},
            "token":"request-token","sign_str":"signature","timestamp":123456789,
            "data":"move-only",
            "resource_group_access":{"resource_group_ids":["16","8","16"],
                "resource_group_has_ids":["8","16","8"],"ungrouped_resource_allowed":true}
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
        assert!(result.resource_group_access.is_none());
    }

    #[test]
    fn resource_group_access_normalizes_and_intersects_without_ambiguous_none() {
        let id = |value| Id::new(value).unwrap();
        let access = ResourceGroupAccess {
            resource_group_ids: Some(vec![id(9), id(3), id(9), id(7)]),
            resource_group_has_ids: vec![id(7), id(5), id(3), id(5)],
            ungrouped_resource_allowed: false,
        };
        assert_eq!(
            access.normalized(),
            ResourceGroupAccess {
                resource_group_ids: Some(vec![id(3), id(7), id(9)]),
                resource_group_has_ids: vec![id(3), id(5), id(7)],
                ungrouped_resource_allowed: false,
            }
        );
        assert_eq!(access.effective_ids(), vec![id(3), id(7)]);

        let all_authorized = ResourceGroupAccess {
            resource_group_ids: None,
            resource_group_has_ids: vec![id(7), id(3), id(7)],
            ungrouped_resource_allowed: false,
        };
        assert_eq!(all_authorized.effective_ids(), vec![id(3), id(7)]);

        let empty_authorized = ResourceGroupAccess {
            resource_group_ids: None,
            resource_group_has_ids: Vec::new(),
            ungrouped_resource_allowed: false,
        };
        assert!(empty_authorized.effective_ids().is_empty());
    }

    #[test]
    fn ungrouped_permission_requires_all_projects_and_defaults_to_false() {
        let old: ResourceGroupAccess = serde_json::from_value(json!({
            "resource_group_has_ids": []
        }))
        .unwrap();
        assert!(!old.allows_ungrouped());
        assert!(serde_json::to_value(&old)
            .unwrap()
            .get("ungrouped_resource_allowed")
            .is_none());

        let mut access = ResourceGroupAccess {
            ungrouped_resource_allowed: true,
            ..old
        };
        assert!(access.allows_ungrouped());
        assert!(access.normalized().allows_ungrouped());
        assert_eq!(
            serde_json::to_value(&access).unwrap()["ungrouped_resource_allowed"],
            json!(true)
        );
        access.resource_group_ids = Some(Vec::new());
        assert!(
            !access.allows_ungrouped(),
            "explicit empty selection is not all projects"
        );
        access.resource_group_ids = Some(vec![Id::new(3).unwrap()]);
        assert!(
            !access.allows_ungrouped(),
            "a selected project excludes ungrouped resources"
        );
    }

    #[test]
    fn resource_group_access_is_optional_but_strict_when_present() {
        let legacy: CommonRequest<String> = serde_json::from_value(json!({
            "version":null,"user_id":7,"merchant":null,"tenant":null,
            "token":null,"sign_str":null,"timestamp":1,"data":"payload"
        }))
        .unwrap();
        assert!(legacy.resource_group_access.is_none());
        assert!(serde_json::to_value(legacy)
            .unwrap()
            .get("resource_group_access")
            .is_none());

        let access: ResourceGroupAccess = serde_json::from_value(json!({
            "resource_group_ids":["9","3"],
            "resource_group_has_ids":["3","5","9"]
        }))
        .unwrap();
        assert_eq!(
            access.effective_ids(),
            vec![Id::new(3).unwrap(), Id::new(9).unwrap()]
        );
        assert!(serde_json::from_value::<ResourceGroupAccess>(json!({
            "resource_group_has_ids":[],"unknown":true
        }))
        .is_err());
        assert!(serde_json::from_value::<ResourceGroupAccess>(json!({
            "resource_group_ids":[]
        }))
        .is_err());
    }

    #[test]
    fn obsolete_envelope_fields_are_not_carried_forward() {
        let legacy = json!({
            "version":"1","user_id":7,
            "merchant":{"id":1,"token":"opaque","merchant_mark":"m"},
            "tenant":{"tenant_id":4,"tenant_type":"team","tenant_name":"old",
                "permission":"{}","is_admin":false},
            "token":"t","sign_str":"s","timestamp":1,"data":"payload"
        });
        let mut obsolete = legacy.clone();
        // Removed fields follow the original envelope's unknown-field behavior,
        // even when their values do not match the retired protocol types.
        for field in [
            "resource_group_id",
            "auth_context",
            "transition_auth_context",
        ] {
            obsolete[field] = json!({"retired":"not an authorization input"});
        }
        for field in [
            "visible_project_ids",
            "scope_all",
            "visibility_state",
            "visibility_version",
            "visibility_lease",
        ] {
            obsolete["tenant"][field] = json!({"retired":true});
        }
        let request: CommonRequest<String> = serde_json::from_value(obsolete).unwrap();
        assert!(request.resource_group_access.is_none());
        assert_eq!(serde_json::to_value(request).unwrap(), legacy);
    }

    #[test]
    fn absent_null_and_empty_access_keep_distinct_request_modes() {
        for access in [None, Some(serde_json::Value::Null)] {
            let mut value = json!({"timestamp":1});
            if let Some(access) = access {
                value["resource_group_access"] = access;
            }
            let request: CommonRequest<String> = serde_json::from_value(value).unwrap();
            assert!(request.resource_group_access.is_none());
            assert!(serde_json::to_value(request)
                .unwrap()
                .get("resource_group_access")
                .is_none());
        }

        let empty = json!({"resource_group_has_ids":[]});
        let request: CommonRequest<String> = serde_json::from_value(json!({
            "timestamp":1, "resource_group_access":empty
        }))
        .unwrap();
        let access = request.resource_group_access.as_ref().unwrap();
        assert!(access.effective_ids().is_empty());
        assert!(!access.allows_ungrouped());
        assert_eq!(
            serde_json::to_value(request).unwrap()["resource_group_access"],
            empty
        );

        for access in [
            json!({}),
            json!({"resource_group_has_ids":[1]}),
            json!({"resource_group_has_ids":["0"]}),
            json!({"resource_group_has_ids":[],"unknown":true}),
        ] {
            assert!(serde_json::from_value::<CommonRequest<String>>(json!({
                "timestamp":1, "resource_group_access":access
            }))
            .is_err());
        }
    }
}
