use thiserror::Error;

#[derive(Debug, Error)]
pub enum ApiError {
    #[error("HTTP {status}: {message}")]
    Http { status: u16, message: String },

    #[error("Network error: {0}")]
    Network(String),

    #[error("Parse error: {0}")]
    Parse(String),
}

impl ApiError {
    #[allow(dead_code)]
    pub fn is_conflict(&self) -> bool {
        matches!(self, Self::Http { status: 409, .. })
    }
}
