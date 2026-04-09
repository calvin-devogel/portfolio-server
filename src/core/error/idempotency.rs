use actix_web::{HttpResponse, ResponseError, http::StatusCode};

use super::response::{AppError, build_error_response, error_chain_fmt};

#[derive(thiserror::Error)]
pub enum IdempotencyError {
    #[error("Missing idempotency key")]
    MissingKey,
    #[error("Invalid idempotency key format")]
    InvalidKey(String),
    #[error("Request is already being processed")]
    InFlight,

    #[error("Database error")]
    Database(#[from] sqlx::Error),
    #[error(transparent)]
    Unexpected(#[from] anyhow::Error),
}

impl std::fmt::Debug for IdempotencyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        error_chain_fmt(self, f)
    }
}

impl AppError for IdempotencyError {
    fn code(&self) -> &'static str {
        match self {
            Self::MissingKey => "missing_idempotency_key",
            Self::InvalidKey(_) => "invalid_idempotency_key",
            Self::InFlight => "request_in_flight",
            Self::Database(_) | Self::Unexpected(_) => "internal_error",
        }
    }

    fn client_message(&self) -> &str {
        match self {
            Self::MissingKey => "An idempotency key is required for this request.",
            Self::InvalidKey(msg) => msg,
            Self::InFlight => {
                "A request with this key is already being processed. Please wait and retry."
            }
            Self::Database(_) | Self::Unexpected(_) => {
                "An unexpected error occurred. Please try again later."
            }
        }
    }

    fn http_status(&self) -> StatusCode {
        match self {
            Self::MissingKey | Self::InvalidKey(_) => StatusCode::BAD_REQUEST,
            Self::InFlight => StatusCode::CONFLICT,
            Self::Database(_) | Self::Unexpected(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

impl ResponseError for IdempotencyError {
    fn status_code(&self) -> StatusCode {
        self.http_status()
    }

    fn error_response(&self) -> HttpResponse {
        build_error_response(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_idempotency_error_codes() {
        let err = IdempotencyError::MissingKey;
        assert_eq!(err.status_code(), StatusCode::BAD_REQUEST);
        let err = IdempotencyError::InvalidKey("Invalid key".to_string());
        assert_eq!(err.status_code(), StatusCode::BAD_REQUEST);
        let err = IdempotencyError::InFlight;
        assert_eq!(err.status_code(), StatusCode::CONFLICT);
        let err = IdempotencyError::Database(sqlx::Error::RowNotFound);
        assert_eq!(err.status_code(), StatusCode::INTERNAL_SERVER_ERROR);
        let err = IdempotencyError::Unexpected(anyhow::anyhow!("Unexpected"));
        assert_eq!(err.status_code(), StatusCode::INTERNAL_SERVER_ERROR);
    }
}
