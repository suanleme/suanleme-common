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

impl<T: Default> CommonRequest<T> {
    pub fn buildr() -> Self {
        CommonRequest::default()
    }
    pub fn user_id(mut self, user_id: Option<String>) -> Self {
        self.user_id = user_id;
        self
    }
    pub fn token(mut self, token: Option<String>) -> Self {
        self.token = token;
        self
    }
    pub fn sign_str(mut self, sign_str: Option<String>) -> Self {
        self.sign_str = sign_str;
        self
    }
    pub fn data(mut self, data: Option<T>) -> Self {
        self.data = data;
        self
    }
}

impl<T: Default> CommonResponse<T> {
    pub fn buildr() -> Self {
        CommonResponse::default()
    }
    pub fn code(mut self, code: String) -> Self {
        self.code = code;
        self
    }
    pub fn message(mut self, message: Option<String>) -> Self {
        self.message = message;
        self
    }
    pub fn data(mut self, data: Option<T>) -> Self {
        self.data = data;
        self
    }
}
