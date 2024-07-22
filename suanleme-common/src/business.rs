use suanleme_macro::Data;

#[derive(Debug, Default, Clone, serde::Serialize, serde::Deserialize, Data)]
pub struct CommonRequest<T: Default> {
    user_id: Option<String>,
    token: Option<String>,
    sign_str: Option<String>,
    data: Option<T>,
}

#[derive(Debug, Default, Clone, serde::Serialize, serde::Deserialize, Data)]
pub struct CommonResponse<T: Default> {
    code: String,
    message: Option<String>,
    data: Option<T>,
}
