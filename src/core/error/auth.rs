use crate::core::error::IdempotencyError;
use actix_web::{HttpResponse, ResponseError, http::StatusCode};

use super::response::{AppError, build_error_response};

#[derive(thiserror::Error, Debug)]
pub enum AuthError {
    #[error("Too many login requests")]
    RateLimited,
    #[error("Invalid credentials")]
    InvalidCredentials(#[source] anyhow::Error),
    #[error("Unauthorized: {0}")]
    Unauthorized(String),
    #[error("Bad request: {0}")]
    BadRequest(String),
    #[error("Forbidden: {0}")]
    Forbidden(String),
    #[error("Conflict: {0}")]
    Conflict(String),
    #[error(transparent)]
    Idempotency(#[from] IdempotencyError),

    #[error("Database error")]
    Database(#[from] sqlx::Error),
    #[error("Session error")]
    SessionInsert(#[from] actix_session::SessionInsertError),
    #[error(transparent)]
    Unexpected(#[from] anyhow::Error),
}

impl AppError for AuthError {
    fn code(&self) -> &'static str {
        match self {
            Self::RateLimited => "rate_limited",
            Self::InvalidCredentials(_) => "invalid_credentials",
            Self::Unauthorized(_) => "unauthorized",
            Self::BadRequest(_) => "bad_request",
            Self::Forbidden(_) => "forbidden",
            Self::Conflict(_) => "conflict",
            Self::Idempotency(e) => e.code(),
            Self::SessionInsert(_) => "session_error",
            Self::Database(_) | Self::Unexpected(_) => "internal_error",
        }
    }

    fn client_message(&self) -> &str {
        match self {
            Self::RateLimited => "Too many login attempts. Please try again later.",
            Self::InvalidCredentials(_) => "Invalid username or password.",
            Self::Unauthorized(msg)
            | Self::BadRequest(msg)
            | Self::Forbidden(msg)
            | Self::Conflict(msg) => msg,
            Self::Database(_) | Self::Unexpected(_) | Self::SessionInsert(_) => {
                "An unexpected error occurred. Please try again later."
            }
            Self::Idempotency(e) => e.client_message(),
        }
    }

    fn http_status(&self) -> StatusCode {
        match self {
            Self::RateLimited => StatusCode::TOO_MANY_REQUESTS,
            Self::InvalidCredentials(_) | Self::Unauthorized(_) => StatusCode::UNAUTHORIZED,
            Self::BadRequest(_) => StatusCode::BAD_REQUEST,
            Self::Forbidden(_) => StatusCode::FORBIDDEN,
            Self::Conflict(_) => StatusCode::CONFLICT,
            Self::Database(_) | Self::Unexpected(_) | Self::SessionInsert(_) => {
                StatusCode::INTERNAL_SERVER_ERROR
            }
            Self::Idempotency(e) => e.http_status(),
        }
    }
}

impl ResponseError for AuthError {
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
    fn test_auth_error_codes() {
        let rate_limited = AuthError::RateLimited;
        assert_eq!(rate_limited.code(), "rate_limited");
        assert_eq!(
            rate_limited.client_message(),
            "Too many login attempts. Please try again later."
        );
        assert_eq!(rate_limited.http_status(), StatusCode::TOO_MANY_REQUESTS);

        let invalid_credentials =
            AuthError::InvalidCredentials(anyhow::anyhow!("Invalid password"));
        assert_eq!(invalid_credentials.code(), "invalid_credentials");
        assert_eq!(
            invalid_credentials.client_message(),
            "Invalid username or password."
        );
        assert_eq!(invalid_credentials.http_status(), StatusCode::UNAUTHORIZED);

        let database_error = AuthError::Database(sqlx::Error::RowNotFound);
        assert_eq!(database_error.code(), "internal_error");
        assert_eq!(
            database_error.client_message(),
            "An unexpected error occurred. Please try again later."
        );
        assert_eq!(
            database_error.http_status(),
            StatusCode::INTERNAL_SERVER_ERROR
        );

        let unexpected_error = AuthError::Unexpected(anyhow::anyhow!("Unexpected"));
        assert_eq!(unexpected_error.code(), "internal_error");
        assert_eq!(
            unexpected_error.client_message(),
            "An unexpected error occurred. Please try again later."
        );
        assert_eq!(
            unexpected_error.http_status(),
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }
}
