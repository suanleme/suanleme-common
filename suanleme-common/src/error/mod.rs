use std::fmt::{self, Display, Formatter};

pub type BoxError = Box<dyn std::error::Error + Send + Sync>;

#[derive(Debug)]
pub struct MessageError(String);

unsafe impl Send for MessageError {}

unsafe impl Sync for MessageError {}

impl Display for MessageError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "MessageError : {}", self.0)
    }
}

impl std::error::Error for MessageError {}

impl From<String> for MessageError {
    fn from(value: String) -> Self {
        MessageError(value.to_owned())
    }
}

impl MessageError {
    pub fn boxed(self) -> BoxError {
        Box::new(self)
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum HttpError {
    Init,
    NotFind,
    Unauthorized,
    Error(String),
}

impl From<HttpError> for u16 {
    fn from(value: HttpError) -> Self {
        match value {
            HttpError::Init => 200,
            HttpError::NotFind => 404,
            HttpError::Unauthorized => 401,
            HttpError::Error(_) => 500,
        }
    }
}
