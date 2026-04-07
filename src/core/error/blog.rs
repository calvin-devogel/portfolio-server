use actix_web::{HttpResponse, ResponseError, http::StatusCode};

use crate::core::error::Idempotency;

use super::response::{AppError, build_error_response};

#[derive(thiserror::Error, Debug)]
pub enum Blog {
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
    Idempotency(#[from] Idempotency),
}

impl AppError for Blog {
    fn code(&self) -> &'static str {
        match self {
            Self::NotFound => "not_found",
            Self::DuplicatePost => "duplicate_post",
            Self::SlugConflict => "slug_conflict",
            Self::Validation(_) => "validation_error",
            Self::BadRequest(_) => "bad_request",
            Self::Database(_) | Self::Unexpected(_) => "internal_error",
            Self::Idempotency(_) => "idempotency_error",
        }
    }

    fn client_message(&self) -> &'static str {
        match self {
            Self::NotFound => "The requested post could not be found.",
            Self::DuplicatePost => "A post with this content already exists.",
            Self::SlugConflict => "A post with this slug already exists.",
            // Validation and BadRequest carry caller-supplied messages that are safe to show
            Self::Validation(_) | Self::BadRequest(_) => "",
            Self::Database(_) | Self::Unexpected(_) | Self::Idempotency(_) => {
                "An unexpected error occurred. Please try again later."
            }
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
            Self::Database(_) | Self::Unexpected(_) | Self::Idempotency(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

impl ResponseError for Blog {
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
        let e = Blog::NotFound;
        assert_eq!(e.status_code(), StatusCode::NOT_FOUND);
        let e = Blog::DuplicatePost;
        assert_eq!(e.status_code(), StatusCode::CONFLICT);
        let e = Blog::SlugConflict;
        assert_eq!(e.status_code(), StatusCode::CONFLICT);
        let e = Blog::Validation("bad content".into());
        assert_eq!(e.status_code(), StatusCode::BAD_REQUEST);
        let e = Blog::BadRequest("no fields provided".into());
        assert_eq!(e.status_code(), StatusCode::BAD_REQUEST);
        let e = Blog::Database(sqlx::Error::RowNotFound);
        assert_eq!(e.status_code(), StatusCode::INTERNAL_SERVER_ERROR);
        let e = Blog::Unexpected(anyhow::anyhow!("unexpected"));
        assert_eq!(e.status_code(), StatusCode::INTERNAL_SERVER_ERROR);
        let e = Blog::Idempotency(Idempotency::InFlight);
        assert_eq!(e.status_code(), StatusCode::INTERNAL_SERVER_ERROR);
    }
}