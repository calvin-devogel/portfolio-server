use actix_web::{HttpResponse, ResponseError, http::StatusCode};

#[derive(serde::Serialize, serde::Deserialize)]
struct ErrorMessage {
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
}

impl ErrorMessage {
    const fn new(message: Option<String>) -> Self {
        Self { message }
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
            Self::MessageLength => Some(ErrorMessage::new(Some(
                "Message must be between 10 and 5000 characters".to_string(),
            ))),
            Self::NameLength => Some(ErrorMessage::new(Some(
                "Name must be between 2 and 100 characters.".to_string(),
            ))),
            Self::RateLimitExceeded | Self::DuplicateMessage | Self::UnexpectedError(_) => None,
        }
    }
}

impl ResponseError for ContactSubmissionError {
    fn status_code(&self) -> StatusCode {
        match self {
            Self::InvalidEmail | Self::MessageLength | Self::NameLength => StatusCode::BAD_REQUEST,
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

#[derive(thiserror::Error, Debug)]
pub enum MessageGetError {
    #[error("Failed to get message count")]
    TotalCount,
}

impl ResponseError for MessageGetError {
    fn status_code(&self) -> StatusCode {
        match self {
            Self::TotalCount => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

#[derive(thiserror::Error, Debug)]
pub enum MessagePatchError {
    #[error("Message not found")]
    MessageNotFound,
    #[error(transparent)]
    UnexpectedError(#[from] anyhow::Error),
}

impl ResponseError for MessagePatchError {
    fn status_code(&self) -> StatusCode {
        match self {
            Self::MessageNotFound => StatusCode::NOT_FOUND,
            Self::UnexpectedError(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn correct_status_code() {
        let e = ContactSubmissionError::InvalidEmail;
        assert_eq!(e.status_code(), StatusCode::BAD_REQUEST);
        let e = ContactSubmissionError::MessageLength;
        assert_eq!(e.status_code(), StatusCode::BAD_REQUEST);
        let e = ContactSubmissionError::NameLength;
        assert_eq!(e.status_code(), StatusCode::BAD_REQUEST);
        let e = ContactSubmissionError::RateLimitExceeded;
        assert_eq!(e.status_code(), StatusCode::TOO_MANY_REQUESTS);
        let e = ContactSubmissionError::DuplicateMessage;
        assert_eq!(e.status_code(), StatusCode::CONFLICT);
        let e = ContactSubmissionError::UnexpectedError(anyhow::anyhow!("Unexpected error"));
        assert_eq!(e.status_code(), StatusCode::INTERNAL_SERVER_ERROR);

        let e = MessageGetError::TotalCount;
        assert_eq!(e.status_code(), StatusCode::INTERNAL_SERVER_ERROR);

        let e = MessagePatchError::MessageNotFound;
        assert_eq!(e.status_code(), StatusCode::NOT_FOUND);
        let e = MessagePatchError::UnexpectedError(anyhow::anyhow!("Unexpected error"));
        assert_eq!(e.status_code(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[test]
    fn correct_error_message() {
        let e = ContactSubmissionError::MessageLength;
        let error_message = e.to_message_error().unwrap();
        assert_eq!(
            error_message.message,
            Some("Message must be between 10 and 5000 characters".to_string())
        );

        let e = ContactSubmissionError::NameLength;
        let error_message = e.to_message_error().unwrap();
        assert_eq!(
            error_message.message,
            Some("Name must be between 2 and 100 characters.".to_string())
        );

        let e = ContactSubmissionError::RateLimitExceeded;
        assert!(e.to_message_error().is_none());

        let e = ContactSubmissionError::DuplicateMessage;
        assert!(e.to_message_error().is_none());

        let e = ContactSubmissionError::UnexpectedError(anyhow::anyhow!("Unexpected error"));
        assert!(e.to_message_error().is_none());
    }
}
