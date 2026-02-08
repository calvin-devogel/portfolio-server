use actix_web::{ResponseError, http::StatusCode, HttpResponse};

#[derive(serde::Serialize)]
struct ErrorMessage {
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>
}

impl ErrorMessage {
    const fn new(
        message: Option<String>,
    ) -> Self {
        Self {message}
    }
}

#[derive(thiserror::Error, Debug)]
pub enum ContactSubmissionError {
    #[error("Invalid email address")]
    InvalidEmail,
    #[error("Message length must be 10-5000 characters")]
    MessageLength,
    #[error("Name length must be 2-100 characters")]
    NameLength,
    #[error("Rate limit exceeded")]
    RateLimitExceeded,
    #[error("Duplicate message detected")]
    DuplicateMessage,
    #[error(transparent)]
    UnexpectedError(#[from] anyhow::Error),
}

impl ContactSubmissionError {
    fn to_message_error(&self) -> Option<ErrorMessage> {
        match self {
            Self::InvalidEmail => Some(ErrorMessage::new(Some("Invalid email".to_string()))),
            Self::MessageLength => Some(ErrorMessage::new(Some("Message must be between 10 and 5000 characters".to_string()))),
            Self::NameLength => Some(ErrorMessage::new(Some("Name must be between 2 and 100 characters.".to_string()))),
            Self::RateLimitExceeded | Self::DuplicateMessage | Self::UnexpectedError(_) => None,
        }
    }
}

impl ResponseError for ContactSubmissionError {
    fn status_code(&self) -> StatusCode {
        match self {
            Self::InvalidEmail | Self::MessageLength | Self::NameLength => {
                StatusCode::BAD_REQUEST
            }
            Self::RateLimitExceeded => StatusCode::TOO_MANY_REQUESTS,
            Self::DuplicateMessage => StatusCode::CONFLICT,
            Self::UnexpectedError(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    fn error_response(&self) -> HttpResponse {
        let status = self.status_code();
        let error = self.to_message_error();
        HttpResponse::build(status).json(error)
    }
}