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
            Self::InvalidContent(_) | Self::BadRequest(_) | Self::ValidationError(_) => {
                StatusCode::BAD_REQUEST
            }
            Self::PostNotFound => StatusCode::NOT_FOUND,
            Self::DuplicatePost | Self::SlugConflict => StatusCode::CONFLICT,
            Self::QueryFailed | Self::UnexpectedError(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn correct_status_code() {
        let e = BlogError::PostNotFound;
        assert_eq!(e.status_code(), StatusCode::NOT_FOUND);
        let e = BlogError::DuplicatePost;
        assert_eq!(e.status_code(), StatusCode::CONFLICT);
        let e = BlogError::SlugConflict;
        assert_eq!(e.status_code(), StatusCode::CONFLICT);
        let e = BlogError::QueryFailed;
        assert_eq!(e.status_code(), StatusCode::INTERNAL_SERVER_ERROR);
        let e = BlogError::UnexpectedError(anyhow::anyhow!("Unexpected error"));
        assert_eq!(e.status_code(), StatusCode::INTERNAL_SERVER_ERROR);
        let e = BlogError::InvalidContent("Invalid content".to_string());
        assert_eq!(e.status_code(), StatusCode::BAD_REQUEST);
        let e = BlogError::BadRequest(anyhow::anyhow!("Bad request"));
        assert_eq!(e.status_code(), StatusCode::BAD_REQUEST);
        let e = BlogError::ValidationError("Validation failed".to_string());
        assert_eq!(e.status_code(), StatusCode::BAD_REQUEST);
    }
}
