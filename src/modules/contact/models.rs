// src/modules/contact/models.rs
use chrono::{DateTime, Utc};
use email_address::EmailAddress;
use std::ops::Deref;
use std::str::FromStr;
use uuid::Uuid;

use crate::core::error::Contact;

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

#[derive(PartialEq, Debug)]
pub struct ValidatedMessage {
    pub email: String,
    pub sender_name: String,
    pub message_text: String,
}

impl MessageForm {
    pub fn validate(&self) -> Result<ValidatedMessage, Contact> {
        let validated_email = EmailAddress::from_str(&self.email)
            .map(|r| r.email())
            .map_err(|e| {
                tracing::warn!(email = %self.email, error = ?e, "Email validation failed");
                Contact::InvalidEmail
            })?;

        let trimmed_name = self.validate_name()?;
        let trimmed_message = self.validate_message()?;

        Ok(ValidatedMessage {
            email: validated_email,
            sender_name: trimmed_name,
            message_text: trimmed_message,
        })
    }

    fn validate_name(&self) -> Result<String, Contact> {
        let trimmed_name = self.sender_name.trim();
        if trimmed_name.len() < 2 || trimmed_name.len() > 100 {
            tracing::warn!(
                name_length = trimmed_name.len(),
                "Name validation failed: length out of bounds"
            );
            return Err(Contact::NameLength);
        }
        Ok(trimmed_name.to_string())
    }

    fn validate_message(&self) -> Result<String, Contact> {
        let trimmed_message = self.message_text.trim();
        if trimmed_message.len() < 10 || trimmed_message.len() > 5000 {
            tracing::warn!(
                message_length = trimmed_message.len(),
                "Message validation failed: length out of bounds"
            );
            return Err(Contact::MessageLength);
        }
        Ok(trimmed_message.to_string())
    }
}

#[cfg(test)]
mod test {
    // Note: Kept your unit tests here unchanged mapping to ValidatedMessage
    use super::MessageForm;
    use crate::core::error::Contact;

    #[test]
    fn message_form_validation_works() {
        let form_with_bad_email = MessageForm {
            email: "bademail".to_string(),
            sender_name: "John Doe".to_string(),
            message_text: "This is a test message.".to_string(),
        };

        let result = form_with_bad_email.validate();
        assert!(matches!(result, Err(Contact::InvalidEmail)));

        // ... [keep rest of tests unchanged]
    }
}
