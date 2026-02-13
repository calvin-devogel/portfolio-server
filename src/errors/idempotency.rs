use actix_web::{ResponseError, http::StatusCode};

#[derive(thiserror::Error, Debug)]
pub enum IdempotencyError {
    #[error("Missing idempotency key")]
    MissingIdempotencyKey,
    #[error("Invalid idempotency key format")]
    InvalidKeyFormat,
    #[error(transparent)]
    UnexpectedError(#[from] anyhow::Error)
}

impl ResponseError for IdempotencyError {
    fn status_code(&self) -> StatusCode {
        match self {
            Self::MissingIdempotencyKey | Self::InvalidKeyFormat => StatusCode::BAD_REQUEST,
            Self::UnexpectedError(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}