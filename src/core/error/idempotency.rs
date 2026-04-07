use actix_web::{HttpResponse, ResponseError, http::StatusCode};

use super::response::{AppError, build_error_response};

#[derive(thiserror::Error, Debug)]
pub enum Idempotency {
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

impl AppError for Idempotency {
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
            Self::InFlight => "A request with this key is already being processed. Please wait and retry.",
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

impl ResponseError for Idempotency {
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
        let err = Idempotency::MissingKey;
        assert_eq!(err.status_code(), StatusCode::BAD_REQUEST);
        let err = Idempotency::InvalidKey("Invalid key".to_string());
        assert_eq!(err.status_code(), StatusCode::BAD_REQUEST);
        let err = Idempotency::InFlight;
        assert_eq!(err.status_code(), StatusCode::CONFLICT);
        let err = Idempotency::Database(sqlx::Error::RowNotFound);
        assert_eq!(err.status_code(), StatusCode::INTERNAL_SERVER_ERROR);
        let err = Idempotency::Unexpected(anyhow::anyhow!("Unexpected"));
        assert_eq!(err.status_code(), StatusCode::INTERNAL_SERVER_ERROR);
    }
}