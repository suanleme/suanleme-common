use suanleme_macro::Data;

use crate::utils::date_util::get_now_date_time_as_millis;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Data)]
pub struct CommonRequest<T: Default> {
    user_id: Option<i64>,
    token: Option<String>,
    sign_str: Option<String>,
    timestamp: u128,
    data: Option<T>,
}

#[derive(Debug, Default, Clone, serde::Serialize, serde::Deserialize, Data)]
pub struct CommonResponse<T: Default> {
    code: String,
    message: Option<String>,
    data: Option<T>,
}

#[derive(Debug, Default, Clone, serde::Serialize, serde::Deserialize, Data)]
pub struct Nil;

impl<T: Default> Default for CommonRequest<T> {
    fn default() -> Self {
        Self {
            user_id: Default::default(),
            token: Default::default(),
            sign_str: Default::default(),
            timestamp: get_now_date_time_as_millis(),
            data: Default::default(),
        }
    }
}
