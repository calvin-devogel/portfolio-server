use actix_web::{HttpResponse, ResponseError, http::StatusCode};

use crate::core::error::IdempotencyError;

use super::response::{AppError, build_error_response};

#[derive(thiserror::Error, Debug)]
pub enum ArticleError {
    // Domain rule errors — safe to surface
    #[error("Post not found")]
    NotFound,
    #[error("Duplicate post")]
    DuplicatePost,
    #[error("Slug conflict")]
    SlugConflict,
    #[error("Validation failed: {0}")]
    Validation(String),
    #[error("Bad request: {0}")]
    BadRequest(String),

    // Internal errors — detail must not reach the client
    #[error("Database error")]
    Database(#[from] sqlx::Error),
    #[error(transparent)]
    Unexpected(#[from] anyhow::Error),
    #[error(transparent)]
    Idempotency(#[from] IdempotencyError),
}

impl AppError for ArticleError {
    fn code(&self) -> &'static str {
        match self {
            Self::NotFound => "not_found",
            Self::DuplicatePost => "duplicate_post",
            Self::SlugConflict => "slug_conflict",
            Self::Validation(_) => "validation_error",
            Self::BadRequest(_) => "bad_request",
            Self::Database(_) | Self::Unexpected(_) => "internal_error",
            Self::Idempotency(e) => e.code(),
        }
    }

    fn client_message(&self) -> &str {
        match self {
            Self::NotFound => "The requested post could not be found.",
            Self::DuplicatePost => "A post with this content already exists.",
            Self::SlugConflict => "A post with this slug already exists.",
            // Validation and BadRequest carry caller-supplied messages that are safe to show
            Self::Validation(_) | Self::BadRequest(_) => "",
            Self::Database(_) | Self::Unexpected(_) => {
                "An unexpected error occurred. Please try again later."
            },
            Self::Idempotency(e) => e.client_message(),
        }
    }

    fn override_message(&self) -> Option<&str> {
        match self {
            Self::Validation(msg) | Self::BadRequest(msg) => Some(msg),
            _ => None,
        }
    }

    fn http_status(&self) -> StatusCode {
        match self {
            Self::Validation(_) | Self::BadRequest(_) => StatusCode::BAD_REQUEST,
            Self::NotFound => StatusCode::NOT_FOUND,
            Self::DuplicatePost | Self::SlugConflict => StatusCode::CONFLICT,
            Self::Database(_) | Self::Unexpected(_) => {
                StatusCode::INTERNAL_SERVER_ERROR
            },
            Self::Idempotency(e) => e.http_status(),
        }
    }
}

impl ResponseError for ArticleError {
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
    fn correct_status_code() {
        let e = ArticleError::NotFound;
        assert_eq!(e.status_code(), StatusCode::NOT_FOUND);
        let e = ArticleError::DuplicatePost;
        assert_eq!(e.status_code(), StatusCode::CONFLICT);
        let e = ArticleError::SlugConflict;
        assert_eq!(e.status_code(), StatusCode::CONFLICT);
        let e = ArticleError::Validation("bad content".into());
        assert_eq!(e.status_code(), StatusCode::BAD_REQUEST);
        let e = ArticleError::BadRequest("no fields provided".into());
        assert_eq!(e.status_code(), StatusCode::BAD_REQUEST);
        let e = ArticleError::Database(sqlx::Error::RowNotFound);
        assert_eq!(e.status_code(), StatusCode::INTERNAL_SERVER_ERROR);
        let e = ArticleError::Unexpected(anyhow::anyhow!("unexpected"));
        assert_eq!(e.status_code(), StatusCode::INTERNAL_SERVER_ERROR);
        let e = ArticleError::Idempotency(IdempotencyError::InFlight);
        assert_eq!(e.status_code(), StatusCode::CONFLICT);
    }
}
