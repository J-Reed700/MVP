#[derive(Debug)]
pub enum ApplicationError {
    Validation(String),
    Unauthorized(String),
    Unavailable(String),
    Unexpected(anyhow::Error),
}

pub type ApplicationResult<T> = Result<T, ApplicationError>;

impl ApplicationError {
    pub fn validation(message: impl Into<String>) -> Self {
        Self::Validation(message.into())
    }
}

impl std::fmt::Display for ApplicationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Validation(message) => write!(f, "{message}"),
            Self::Unauthorized(message) => write!(f, "{message}"),
            Self::Unavailable(message) => write!(f, "{message}"),
            Self::Unexpected(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for ApplicationError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Validation(_) => None,
            Self::Unauthorized(_) => None,
            Self::Unavailable(_) => None,
            Self::Unexpected(error) => Some(error.as_ref()),
        }
    }
}

impl From<anyhow::Error> for ApplicationError {
    fn from(error: anyhow::Error) -> Self {
        Self::Unexpected(error)
    }
}
