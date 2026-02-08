use actix_web::{HttpResponse, HttpRequest, web};
use email_address::EmailAddress;
use sqlx::{PgPool, Postgres, Transaction};
use std::str::FromStr;
use uuid::Uuid;

use crate::idempotency::{IdempotencyKey, try_processing, save_response, NextAction};
use crate::errors::ContactSubmissionError;

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
                    "Email validation failed'"
                );
                ContactSubmissionError::InvalidEmail
            })?;

        
        let trimmed_name = self.validate_name()?;
        let trimmed_message = self.validate_message()?;

        Ok(ValidatedMessage {
            email: validated_email,
            sender_name: trimmed_name,
            message_text: trimmed_message
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
    skip(message, pool, request),
    fields(email = %message.email)
)]
pub async fn post_message(
    message: web::Form<MessageForm>,
    pool: web::Data<PgPool>,
    request: HttpRequest,
) -> Result<HttpResponse, actix_web::Error> {
    // get the idempotency key (generated client-side)
    let idempotency_key: IdempotencyKey = request
        .headers()
        .get("Idempotency-Key")
        .and_then(|header| header.to_str().ok())
        .ok_or_else(|| {
            tracing::warn!("Missing Idempotency-Key header");
            ContactSubmissionError::UnexpectedError(anyhow::anyhow!("Missing idempotency key"))
        })?
        .to_string()
        .try_into()
        .map_err(|e| {
            tracing::warn!(error = ?e, "Invalid idempotency key format");
            ContactSubmissionError::UnexpectedError(anyhow::anyhow!("Invalid idempotency key"))
        })?;
    
    let validated_input = message.0.validate()?;

    let next_action = try_processing(&pool, &idempotency_key, None)
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
        NextAction::StartProcessing(transaction) => {
            process_new_message(transaction, &pool, &idempotency_key, validated_input).await
        }
    }
    
}

// consume the transaction immediately for Send safety
async fn process_new_message(
    transaction: Transaction<'static, Postgres>,
    pool: &PgPool,
    idempotency_key: &IdempotencyKey,
    validated_input: ValidatedMessage
) -> Result<HttpResponse, actix_web::Error> {
    let rate_ok = sqlx::query_scalar!(
        "SELECT check_email_rate_limit($1, $2, $3)",
        validated_input.email,
        3,
        60
    )
    .fetch_one(pool)
    .await
    .map_err(|e| ContactSubmissionError::UnexpectedError(
        anyhow::anyhow!("Unexpected error: {e:?}")
    ))?
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
            let response = HttpResponse::Accepted().json(MessageResponse {
                message: "Message recieved successfully",
                message_id
            });

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