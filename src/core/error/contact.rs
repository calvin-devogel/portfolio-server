use actix_web::{HttpResponse, ResponseError, http::StatusCode};
use uuid::Uuid;

use crate::core::error::IdempotencyError;

use super::response::{AppError, build_error_response, error_chain_fmt};

#[derive(thiserror::Error)]
pub enum ContactError {
    // validation errors, safe to surface
    #[error("Invalid email address")]
    InvalidEmail,
    #[error("Message must be between 10 and 5000 characters")]
    MessageLength,
    #[error("Name must be between 2 and 100 characters")]
    NameLength,

    // domain rule errors, also safe to surface
    #[error("Rate limit exceeded")]
    RateLimited,
    #[error("Duplicate message")]
    Duplicate,
    #[error("Message not found: {0}")]
    NotFound(Uuid),

    // internal errors, detail must not reach the client
    #[error("Database error")]
    Database(#[from] sqlx::Error),
    #[error(transparent)]
    Unexpected(#[from] anyhow::Error),
    #[error(transparent)]
    Idempotency(#[from] IdempotencyError),
}

impl std::fmt::Debug for ContactError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        error_chain_fmt(self, f)
    }
}

impl AppError for ContactError {
    fn code(&self) -> &'static str {
        match self {
            Self::InvalidEmail => "invalid_email",
            Self::MessageLength => "message_length",
            Self::NameLength => "name_length",
            Self::RateLimited => "rate_limited",
            Self::Duplicate => "duplicate_message",
            Self::NotFound(_) => "not_found",
            Self::Database(_) | Self::Unexpected(_) => "internal_error",
            Self::Idempotency(e) => e.code(),
        }
    }

    fn client_message(&self) -> &str {
        match self {
            Self::InvalidEmail => "Please provide a valid email address.",
            Self::MessageLength => "Message must be between 10 and 5000 characters.",
            Self::NameLength => "Name must be between 2 and 100 characters.",
            Self::RateLimited => "You have sent too many messages. Please try again later.",
            Self::Duplicate => "This message has already been sent.",
            Self::NotFound(_) => "The requested message could not be found.",
            Self::Database(_) | Self::Unexpected(_) => {
                "An unexpected error occurred. Please try again later."
            }
            Self::Idempotency(e) => e.client_message(),
        }
    }

    fn http_status(&self) -> StatusCode {
        match self {
            Self::InvalidEmail | Self::MessageLength | Self::NameLength => StatusCode::BAD_REQUEST,
            Self::RateLimited => StatusCode::TOO_MANY_REQUESTS,
            Self::Duplicate => StatusCode::CONFLICT,
            Self::NotFound(_) => StatusCode::NOT_FOUND,
            Self::Database(_) | Self::Unexpected(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::Idempotency(e) => e.http_status(),
        }
    }
}

impl ResponseError for ContactError {
    fn status_code(&self) -> StatusCode {
        self.http_status()
    }

    fn error_response(&self) -> HttpResponse {
        build_error_response(self)
    }
}
