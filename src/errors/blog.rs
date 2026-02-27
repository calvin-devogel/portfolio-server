use actix_web::{ResponseError, http::StatusCode};

#[derive(thiserror::Error, Debug)]
pub enum BlogError {
    #[error("Query failed")]
    QueryFailed,
    #[error("Post not found")]
    PostNotFound,
    #[error("Bad request")]
    BadRequest(#[source] anyhow::Error),
    #[error("Invalid blog post content: {0}")]
    InvalidContent(String),
    #[error("Duplicate post")]
    DuplicatePost,
    #[error("Slug conflict")]
    SlugConflict,
    #[error("Form validation failed")]
    ValidationError(String),
    #[error(transparent)]
    UnexpectedError(#[from] anyhow::Error),
}

impl ResponseError for BlogError {
    fn status_code(&self) -> StatusCode {
        match self {
            Self::InvalidContent(_) | Self::BadRequest(_) | Self::ValidationError(_) => StatusCode::BAD_REQUEST,
            Self::PostNotFound => StatusCode::NOT_FOUND,
            Self::DuplicatePost | Self::SlugConflict => StatusCode::CONFLICT,
            Self::QueryFailed | Self::UnexpectedError(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}
