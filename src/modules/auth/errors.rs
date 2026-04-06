use actix_web::{ResponseError, http::StatusCode};

#[derive(thiserror::Error, Debug)]
pub enum AuthError {
    #[error("Too many login requests")]
    RateLimitExceeded,
    #[error("Invalid credentials")]
    InvalidCredentials(#[source] anyhow::Error),
    #[error(transparent)]
    UnexpectedError(#[from] anyhow::Error),
}

impl ResponseError for AuthError {
    fn status_code(&self) -> StatusCode {
        match self {
            Self::RateLimitExceeded => StatusCode::TOO_MANY_REQUESTS,
            Self::InvalidCredentials(_) => StatusCode::UNAUTHORIZED,
            Self::UnexpectedError(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn correct_status_code() {
        let e = AuthError::RateLimitExceeded;
        assert_eq!(e.status_code(), StatusCode::TOO_MANY_REQUESTS);
        let e = AuthError::InvalidCredentials(anyhow::anyhow!("e"));
        assert_eq!(e.status_code(), StatusCode::UNAUTHORIZED);
        let e = AuthError::UnexpectedError(anyhow::anyhow!("e"));
        assert_eq!(e.status_code(), StatusCode::INTERNAL_SERVER_ERROR);
    }
}
