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

    pub fn get_user_id(&self) -> Option<&str> {
        self.user_id.as_deref()
    }
    pub fn get_token(&self) -> Option<&str> {
        self.token.as_deref()
    }
    pub fn get_sign_str(&self) -> Option<&str> {
        self.sign_str.as_deref()
    }
    pub fn get_data(&self) -> Option<&T> {
        self.data.as_ref()
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

    pub fn get_code(&self) -> &str {
        &self.code
    }
    pub fn get_message(&self) -> Option<&str> {
        self.message.as_deref()
    }
    pub fn get_data(&self) -> Option<&T> {
        self.data.as_ref()
    }
}
