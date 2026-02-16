use actix_web::{ResponseError, http::StatusCode};

#[derive(thiserror::Error, Debug)]
pub enum MetricsError {
    #[error(transparent)]
    UnexpectedError(#[from] anyhow::Error),
}

impl ResponseError for MetricsError {
 fn status_code(&self) -> StatusCode {
    match self {
        Self::UnexpectedError(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
 }
}