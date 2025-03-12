use suanleme_macro::Data;

use crate::utils::date_util::get_now_date_time_as_millis;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Data)]
pub struct MerchantCommonRequest<T> {
    pub version: Option<String>,
    pub user_id: Option<i64>,
    pub merchant: Option<Merchant>,
    pub timestamp: u128,
    pub data: Option<T>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Data)]
pub struct CommonRequest<T> {
    pub version: Option<String>,
    pub user_id: Option<i64>,
    pub tenant: Option<Tenant>,
    pub token: Option<String>,
    pub sign_str: Option<String>,
    pub timestamp: u128,
    pub data: Option<T>,
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
            tenant: Default::default(),
            token: Default::default(),
            sign_str: Default::default(),
            timestamp: get_now_date_time_as_millis(),
            data: None,
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
