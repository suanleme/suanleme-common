#[derive(Debug, Default, Clone, serde::Serialize, serde::Deserialize)]
pub struct CommonRequest<T> {
    user_id: Option<String>,
    token: Option<String>,
    sign_str: Option<String>,
    data: Option<T>,
}

#[derive(Debug, Default, Clone, serde::Serialize, serde::Deserialize)]
pub struct CommonResponse<T> {
    code: String,
    message: Option<String>,
    data: Option<T>,
}
