use actix_web::{HttpRequest, HttpResponse, web};
use email_address::EmailAddress;
use sqlx::{PgPool, Postgres, Transaction};
use std::str::FromStr;
use uuid::Uuid;

use crate::configuration::MessageRateLimitSettings;
use crate::errors::ContactSubmissionError;
use crate::idempotency::{IdempotencyKey, NextAction, save_response, try_processing, get_idempotency_key};

#[derive(serde::Deserialize)]
pub struct MessageForm {
    email: String,
    sender_name: String,
    message_text: String,
}

#[derive(serde::Serialize)]
struct MessageResponse {
    message: &'static str,
    message_id: Uuid,
}

impl MessageResponse {
    pub const fn new(message: &'static str, message_id: Uuid) -> Self {
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
    fields(email = %message.email)
)]
pub async fn post_message(
    message: web::Form<MessageForm>,
    pool: web::Data<PgPool>,
    request: HttpRequest,
    message_config: web::Data<MessageRateLimitSettings>,
) -> Result<HttpResponse, actix_web::Error> {
    // get the idempotency key (generated client-side)
    let idempotency_key: IdempotencyKey = get_idempotency_key(request)
        .map_err(|e| {
            tracing::warn!(error = ?e, "Failed to get idempotency key");
            ContactSubmissionError::UnexpectedError(anyhow::anyhow!("Failed to get idempotency key"))
        })?;

    let validated_input = message.0.validate()?;

    let (next_action, transaction) = try_processing(&pool, &idempotency_key, None)
        .await
        .map_err(|e| {
            tracing::warn!(error = ?e, "Idempotent processing failed");
            ContactSubmissionError::UnexpectedError(anyhow::anyhow!("Idempotent processing failed"))
        })?;

    match next_action {
        NextAction::ReturnSavedResponse(saved_response) => {
            tracing::info!("Returning saved response for idempotent request");
            Ok(saved_response)
        }
        NextAction::StartProcessing => {
            let transaction = transaction.expect("Transaction must be present for StartProcessing");
            process_new_message(
                transaction,
                &pool,
                &idempotency_key,
                validated_input,
                &message_config,
            )
            .await
        }
    }
}

#[allow(clippy::future_not_send)]
// consume the transaction immediately for Send safety
async fn process_new_message(
    transaction: Transaction<'static, Postgres>,
    pool: &PgPool,
    idempotency_key: &IdempotencyKey,
    validated_input: ValidatedMessage,
    config: &MessageRateLimitSettings,
) -> Result<HttpResponse, actix_web::Error> {
    let rate_ok = sqlx::query_scalar!(
        "SELECT check_email_rate_limit($1, $2, $3)",
        validated_input.email,
        i32::try_from(config.max_messages).expect("Failed to cast config.max_messages"),
        i32::try_from(config.window_minutes).expect("Failed to cast config.window_minutes")
    )
    .fetch_one(pool)
    .await
    .map_err(|e| {
        ContactSubmissionError::UnexpectedError(anyhow::anyhow!("Unexpected error: {e:?}"))
    })?
    .unwrap_or(false);

    if !rate_ok {
        return Err(ContactSubmissionError::RateLimitExceeded.into());
    }

    let message_id = Uuid::new_v4();
    let result = sqlx::query!(
        r#"
        INSERT INTO messages(message_id, email, sender_name, message_text, created_at, read_message)
        VALUES ($1, $2, $3, $4, NOW(), FALSE)
        "#,
        message_id,
        validated_input.email,
        validated_input.sender_name,
        validated_input.message_text
    )
    .execute(pool)
    .await;

    match result {
        Ok(_) => {
            tracing::info!("Message saved successfully with: {}", message_id);
            let response = HttpResponse::Accepted().json(MessageResponse::new(
                "Message received successfully",
                message_id,
            ));

            let saved_response = save_response(transaction, idempotency_key, None, response)
                .await
                .map_err(ContactSubmissionError::UnexpectedError)?;

            Ok(saved_response)
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
