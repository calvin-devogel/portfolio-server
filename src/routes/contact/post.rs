use actix_web::{HttpRequest, HttpResponse, web};
use email_address::EmailAddress;
use sqlx::PgPool;
use std::ops::Deref;
use std::str::FromStr;
use uuid::Uuid;

use crate::configuration::MessageRateLimitSettings;
use crate::errors::ContactSubmissionError;
use crate::idempotency::execute_idempotent;

#[derive(serde::Deserialize)]
pub struct MessageForm {
    email: String,
    sender_name: String,
    message_text: String,
}

#[derive(Clone, Copy, Debug, serde::Serialize)]
pub struct MessageId(Uuid);

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
struct MessageResponse {
    message: &'static str,
    message_id: MessageId,
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
struct ValidatedMessage {
    email: String,
    sender_name: String,
    message_text: String,
}

impl MessageForm {
    fn validate(&self) -> Result<ValidatedMessage, ContactSubmissionError> {
        let validated_email = EmailAddress::from_str(&self.email)
            .map(|r| r.email())
            .map_err(|e| {
                tracing::warn!(
                    email = %self.email,
                    error = ?e,
                    "Email validation failed"
                );
                ContactSubmissionError::InvalidEmail
            })?;

        let trimmed_name = self.validate_name()?;
        let trimmed_message = self.validate_message()?;

        Ok(ValidatedMessage {
            email: validated_email,
            sender_name: trimmed_name,
            message_text: trimmed_message,
        })
    }

    fn validate_name(&self) -> Result<String, ContactSubmissionError> {
        let trimmed_name = self.sender_name.trim();
        if trimmed_name.len() < 2 || trimmed_name.len() > 100 {
            tracing::warn!(
                name_length = trimmed_name.len(),
                "Name validation failed: length out of bounds"
            );
            return Err(ContactSubmissionError::NameLength);
        }

        Ok(trimmed_name.to_string())
    }

    fn validate_message(&self) -> Result<String, ContactSubmissionError> {
        let trimmed_message = self.message_text.trim();
        if trimmed_message.len() < 10 || trimmed_message.len() > 5000 {
            tracing::warn!(
                message_length = trimmed_message.len(),
                "Message validation failed: length out of bound"
            );
            return Err(ContactSubmissionError::MessageLength);
        }

        Ok(trimmed_message.to_string())
    }
}

#[tracing::instrument(
    name = "Send message to contact table",
    skip(message, pool, request, message_config),
    fields(
        email = %message.email,
        message_id = tracing::field::Empty
    )
)]
pub async fn post_message(
    message: web::Form<MessageForm>,
    pool: web::Data<PgPool>,
    request: HttpRequest,
    message_config: web::Data<MessageRateLimitSettings>,
) -> Result<HttpResponse, actix_web::Error> {
    let message_to_post = message.0;
    let pool_for_op = pool.clone();
    let config_for_op = message_config.clone();

    execute_idempotent(&request, &pool, None, move || {
        let pool_for_op = pool_for_op.clone();
        let config_for_op = config_for_op.clone();
        async move { process_new_message(&pool_for_op, config_for_op.get_ref(), message_to_post).await }
    })
    .await
}

#[allow(clippy::future_not_send)]
// consume the transaction immediately for Send safety
async fn process_new_message(
    pool: &PgPool,
    config: &MessageRateLimitSettings,
    message: MessageForm
) -> Result<HttpResponse, actix_web::Error> {
    let validated_input = message.validate()?;

    let rate_ok = sqlx::query_scalar!(
        "SELECT check_email_rate_limit($1, $2, $3)",
        &validated_input.email,
        i32::try_from(config.max_messages).expect("Failed to cast config.max_messages"),
        i32::try_from(config.window_minutes).expect("Failed to cast config.window_minutes")
    )
    .fetch_one(pool)
    .await
    .map_err(|e| ContactSubmissionError::UnexpectedError(anyhow::anyhow!("Unexpected error: {e:?}")))?
    .unwrap_or(false);

    if !rate_ok {
        return Err(ContactSubmissionError::RateLimitExceeded.into());
    }

    let message_id = MessageId(Uuid::new_v4());
    tracing::Span::current().record("message_id", tracing::field::display(&message_id));

    let result = sqlx::query!(
        r#"
        INSERT INTO messages(message_id, email, sender_name, message_text, created_at, read_message)
        VALUES ($1, $2, $3, $4, NOW(), FALSE)
        "#,
        *message_id,
        validated_input.email,
        validated_input.sender_name,
        validated_input.message_text
    )
    .execute(pool)
    .await;

    match result {
        Ok(_) => {
            tracing::info!("Message saved successfully with: {}", message_id);
            Ok(HttpResponse::Accepted().json(MessageResponse::new(
                "Message received successfully",
                message_id
            )))
        }
        Err(e) => {
            if e.to_string().contains("Duplicate message detected") {
                tracing::warn!("Duplicate message detected");
                Err(ContactSubmissionError::DuplicateMessage.into())
            } else {
                tracing::error!("Failed to save message: {e:?}");
                Err(ContactSubmissionError::UnexpectedError(e.into()).into())
            }
        }
    }
}

// unit tests
#[cfg(test)]
mod test {
    use super::MessageForm;
    use crate::errors::ContactSubmissionError;

    #[test]
    fn message_form_validation_works() {
        let form_with_bad_email = MessageForm {
            email: "bademail".to_string(),
            sender_name: "John Doe".to_string(),
            message_text: "This is a test message.".to_string(),
        };

        let mut result = form_with_bad_email.validate();
        assert!(matches!(result, Err(ContactSubmissionError::InvalidEmail)));

        let form_with_bad_name = MessageForm {
            email: "test@email.com".to_string(),
            sender_name: "N".to_string(),
            message_text: "This is a test message".to_string(),
        };

        result = form_with_bad_name.validate();
        assert!(matches!(result, Err(ContactSubmissionError::NameLength)));

        let form_with_whitespace_name = MessageForm {
            email: "test@email.com".to_string(),
            sender_name: "   ".to_string(),
            message_text: "This is a test message".to_string(),
        };

        result = form_with_whitespace_name.validate();
        assert!(matches!(result, Err(ContactSubmissionError::NameLength)));

        let form_with_bad_message = MessageForm {
            email: "test@email.com".to_string(),
            sender_name: "John Doe".to_string(),
            message_text: "T".to_string(),
        };

        result = form_with_bad_message.validate();
        assert!(matches!(result, Err(ContactSubmissionError::MessageLength)));

        let good_form = MessageForm {
            email: "test@email.com".to_string(),
            sender_name: "John Doe".to_string(),
            message_text: "This is a test message".to_string(),
        }
        .validate();

        assert!(good_form.is_ok());
    }
}
