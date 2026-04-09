use actix_web::{HttpResponse, ResponseError, http::StatusCode};

use crate::core::error::AuthError;

use super::response::{AppError, build_error_response, error_chain_fmt};

#[derive(thiserror::Error)]
pub enum ChatError {
    #[error("JWT generation failed: {0}")]
    JwtGeneration(String),
    #[error("User not found")]
    UserNotFound(#[from] AuthError),

    #[error("Database error")]
    Database(#[from] sqlx::Error),
    #[error(transparent)]
    Unexpected(#[from] anyhow::Error),
}

impl std::fmt::Debug for ChatError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        error_chain_fmt(self, f)
    }
}

impl AppError for ChatError {
    fn code(&self) -> &'static str {
        match self {
            Self::JwtGeneration(_) => "jwt_generation_failed",
            Self::UserNotFound(_) => "user_not_found",
            Self::Database(_) => "database_error",
            Self::Unexpected(_) => "internal_error",
        }
    }

    fn client_message(&self) -> &'static str {
        match self {
            Self::JwtGeneration(_) | Self::Database(_) | Self::Unexpected(_) => {
                "An unexpected error occurred. Please try again later."
            }
            Self::UserNotFound(_) => "The specified user could not be found.",
        }
    }

    fn override_message(&self) -> Option<&str> {
        None
    }

    fn http_status(&self) -> StatusCode {
        match self {
            Self::JwtGeneration(_) | Self::Database(_) | Self::Unexpected(_) => {
                StatusCode::INTERNAL_SERVER_ERROR
            }
            Self::UserNotFound(_) => StatusCode::NOT_FOUND,
        }
    }
}

impl ResponseError for ChatError {
    fn status_code(&self) -> StatusCode {
        self.http_status()
    }

    fn error_response(&self) -> HttpResponse {
        build_error_response(self)
    }
}
