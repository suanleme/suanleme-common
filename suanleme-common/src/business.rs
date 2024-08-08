use suanleme_macro::Data;

use crate::utils::date_util::get_now_date_time_as_millis;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Data)]
pub struct CommonRequest<T> {
    version: Option<String>,
    user_id: Option<i64>,
    token: Option<String>,
    sign_str: Option<String>,
    timestamp: u128,
    data: Option<T>,
}

#[derive(Debug, Default, Clone, serde::Serialize, serde::Deserialize, Data)]
pub struct CommonResponse<T> {
    code: String,
    message: Option<String>,
    data: Option<T>,
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

impl<T: serde::Serialize> From<CommonResponse<T>> for serde_json::Value {
    fn from(value: CommonResponse<T>) -> Self {
        serde_json::to_value(&value).unwrap()
    }
}

#[derive(Debug, Default, Clone, serde::Serialize, serde::Deserialize, Data)]
pub struct Nil;

impl<T: Default> Default for CommonRequest<T> {
    fn default() -> Self {
        Self {
            version: Default::default(),
            user_id: Default::default(),
            token: Default::default(),
            sign_str: Default::default(),
            timestamp: get_now_date_time_as_millis(),
            data: Default::default(),
        }
    }
}
