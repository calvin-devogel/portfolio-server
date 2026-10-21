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

#[cfg(test)]
mod test {
    use super::*;
    use crate::core::error::AuthError;

    #[test]
    fn jwt_generation_error_is_internal_server_error() {
        let err = ChatError::JwtGeneration("Key parse failed".to_string());
        assert_eq!(err.code(), "jwt_generation_failed");
        assert_eq!(err.status_code(), StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(
            err.client_message(),
            "An unexpected error occurred. Please try again later."
        );
    }

    #[test]
    fn user_not_found_error_is_not_found() {
        let err = ChatError::UserNotFound(AuthError::Database(sqlx::Error::RowNotFound));
        assert_eq!(err.code(), "user_not_found");
        assert_eq!(err.status_code(), StatusCode::NOT_FOUND);
        assert_eq!(
            err.client_message(),
            "The specified user could not be found."
        );
    }

    #[test]
    fn database_error_is_internal_server_error() {
        let err = ChatError::Database(sqlx::Error::RowNotFound);
        assert_eq!(err.code(), "database_error");
        assert_eq!(err.status_code(), StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(
            err.client_message(),
            "An unexpected error occurred. Please try again later."
        );
    }

    #[test]
    fn unexpected_error_is_internal_server_error() {
        let err = ChatError::Unexpected(anyhow::anyhow!("Something went wrong"));
        assert_eq!(err.code(), "internal_error");
        assert_eq!(err.status_code(), StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(
            err.client_message(),
            "An unexpected error occurred. Please try again later."
        );
    }
}
