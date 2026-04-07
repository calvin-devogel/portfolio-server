use actix_web::{HttpResponse, ResponseError, http::StatusCode};

use super::response::{AppError, build_error_response};

#[derive(thiserror::Error, Debug)]
pub enum Auth {
    #[error("Too many login requests")]
    RateLimited,
    #[error("Invalid credentials")]
    InvalidCredentials(#[source] anyhow::Error),
    
    #[error("Database error")]
    Database(#[from] sqlx::Error),
    #[error(transparent)]
    Unexpected(#[from] anyhow::Error),
}

impl AppError for Auth {
    fn code(&self) -> &'static str {
        match self {
            Self::RateLimited => "rate_limited",
            Self::InvalidCredentials(_) => "invalid_credentials",
            Self::Database(_) | Self::Unexpected(_) => "internal_error",
        }
    }

    fn client_message(&self) -> &'static str {
        match self {
            Self::RateLimited => "Too many login attempts. Please try again later.",
            Self::InvalidCredentials(_) => "Invalid username or password.",
            Self::Database(_) | Self::Unexpected(_) => {
                "An unexpected error occurred. Please try again later."
            }
        }
    }

    fn http_status(&self) -> StatusCode {
        match self {
            Self::RateLimited => StatusCode::TOO_MANY_REQUESTS,
            Self::InvalidCredentials(_) => StatusCode::UNAUTHORIZED,
            Self::Database(_) | Self::Unexpected(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

impl ResponseError for Auth {
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
        let rate_limited = Auth::RateLimited;
        assert_eq!(rate_limited.code(), "rate_limited");
        assert_eq!(rate_limited.client_message(), "Too many login attempts. Please try again later.");
        assert_eq!(rate_limited.http_status(), StatusCode::TOO_MANY_REQUESTS);

        let invalid_credentials = Auth::InvalidCredentials(anyhow::anyhow!("Invalid password"));
        assert_eq!(invalid_credentials.code(), "invalid_credentials");
        assert_eq!(invalid_credentials.client_message(), "Invalid username or password.");
        assert_eq!(invalid_credentials.http_status(), StatusCode::UNAUTHORIZED);

        let database_error = Auth::Database(sqlx::Error::RowNotFound);
        assert_eq!(database_error.code(), "internal_error");
        assert_eq!(database_error.client_message(), "An unexpected error occurred. Please try again later.");
        assert_eq!(database_error.http_status(), StatusCode::INTERNAL_SERVER_ERROR);

        let unexpected_error = Auth::Unexpected(anyhow::anyhow!("Unexpected"));
        assert_eq!(unexpected_error.code(), "internal_error");
        assert_eq!(unexpected_error.client_message(), "An unexpected error occurred. Please try again later.");
        assert_eq!(unexpected_error.http_status(), StatusCode::INTERNAL_SERVER_ERROR);
    }
}