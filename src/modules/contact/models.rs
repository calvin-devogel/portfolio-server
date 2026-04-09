// src/modules/contact/models.rs
use chrono::{DateTime, Utc};
use email_address::EmailAddress;
use std::ops::Deref;
use std::str::FromStr;
use uuid::Uuid;

use crate::core::error::ContactError;

#[derive(serde::Serialize)]
pub struct MessageRecord {
    pub message_id: Uuid,
    pub email: String,
    pub sender_name: String,
    pub message_text: String,
    pub created_at: DateTime<Utc>,
    pub read_message: Option<bool>,
}

#[derive(serde::Serialize)]
pub struct MessagesResponse {
    pub messages: Vec<MessageRecord>,
    pub page: i64,
    pub page_size: i64,
    pub total_items: i64,
    pub total_pages: i64,
}

#[derive(serde::Deserialize)]
pub struct MessagePatchRequest {
    pub message_id: Uuid,
    pub read: bool,
}

#[derive(serde::Deserialize)]
pub struct MessageForm {
    pub email: String,
    pub sender_name: String,
    pub message_text: String,
}

#[derive(Clone, Copy, Debug, serde::Serialize)]
pub struct MessageId(pub Uuid);

impl std::fmt::Display for MessageId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl Deref for MessageId {
    type Target = Uuid;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(serde::Serialize)]
pub struct MessageResponse {
    pub message: &'static str,
    pub message_id: MessageId,
}

impl MessageResponse {
    pub const fn new(message: &'static str, message_id: MessageId) -> Self {
        Self {
            message,
            message_id,
        }
    }
}

#[derive(PartialEq, Eq, Debug)]
pub struct ValidatedMessage {
    pub email: String,
    pub sender_name: String,
    pub message_text: String,
}

impl MessageForm {
    pub fn validate(&self) -> Result<ValidatedMessage, ContactError> {
        let validated_email = EmailAddress::from_str(&self.email)
            .map(|r| r.email())
            .map_err(|e| {
                tracing::warn!(email = %self.email, error = ?e, "Email validation failed");
                ContactError::InvalidEmail
            })?;

        let trimmed_name = self.validate_name()?;
        let trimmed_message = self.validate_message()?;

        Ok(ValidatedMessage {
            email: validated_email,
            sender_name: trimmed_name,
            message_text: trimmed_message,
        })
    }

    fn validate_name(&self) -> Result<String, ContactError> {
        let trimmed_name = self.sender_name.trim();
        if trimmed_name.len() < 2 || trimmed_name.len() > 100 {
            tracing::warn!(
                name_length = trimmed_name.len(),
                "Name validation failed: length out of bounds"
            );
            return Err(ContactError::NameLength);
        }
        Ok(trimmed_name.to_string())
    }

    fn validate_message(&self) -> Result<String, ContactError> {
        let trimmed_message = self.message_text.trim();
        if trimmed_message.len() < 10 || trimmed_message.len() > 5000 {
            tracing::warn!(
                message_length = trimmed_message.len(),
                "Message validation failed: length out of bounds"
            );
            return Err(ContactError::MessageLength);
        }
        Ok(trimmed_message.to_string())
    }
}

// unit tests
#[cfg(test)]
mod test {
    use super::MessageForm;
    use crate::core::error::ContactError;

    #[test]
    fn message_form_validation_works() {
        let form_with_bad_email = MessageForm {
            email: "bademail".to_string(),
            sender_name: "John Doe".to_string(),
            message_text: "This is a test message.".to_string(),
        };

        let mut result = form_with_bad_email.validate();
        assert!(matches!(result, Err(ContactError::InvalidEmail)));

        let form_with_bad_name = MessageForm {
            email: "test@email.com".to_string(),
            sender_name: "N".to_string(),
            message_text: "This is a test message".to_string(),
        };

        result = form_with_bad_name.validate();
        assert!(matches!(result, Err(ContactError::NameLength)));

        let form_with_whitespace_name = MessageForm {
            email: "test@email.com".to_string(),
            sender_name: "   ".to_string(),
            message_text: "This is a test message".to_string(),
        };

        result = form_with_whitespace_name.validate();
        assert!(matches!(result, Err(ContactError::NameLength)));

        let form_with_bad_message = MessageForm {
            email: "test@email.com".to_string(),
            sender_name: "John Doe".to_string(),
            message_text: "T".to_string(),
        };

        result = form_with_bad_message.validate();
        assert!(matches!(result, Err(ContactError::MessageLength)));

        let good_form = MessageForm {
            email: "test@email.com".to_string(),
            sender_name: "John Doe".to_string(),
            message_text: "This is a test message".to_string(),
        }
        .validate();

        assert!(good_form.is_ok());
    }

    #[test]
    fn too_long_validation_works() {
        let long_name = MessageForm {
            email: "test@email.com".to_string(),
            sender_name: "a".repeat(101),
            message_text: "a".repeat(10),
        };

        let result = &long_name.validate_name();
        assert!(result.is_err());

        let long_message = MessageForm {
            email: "test@email.com".to_string(),
            sender_name: "a".repeat(10),
            message_text: "a".repeat(5001),
        };

        let result = &long_message.validate_message();
        assert!(result.is_err());
    }
}
