use actix_web::{HttpResponse, HttpRequest, http::StatusCode, ResponseError, web};
use emval::{validate_email, ValidationError};
use sqlx::PgPool;
use uuid::Uuid;

use crate::idempotency::{IdempotencyKey, try_processing, save_response, NextAction};

#[derive(thiserror::Error, Debug)]
pub enum MessageError {
    #[error("Invalid email address")]
    InvalidEmail(#[source] anyhow::Error),
    #[error("Message does not fit length constraints (10-5000 characters)")]
    MessageLengthError(#[source] anyhow::Error),
    #[error("Name does not fit length constraints (2-100 characters)")]
    NameLengthError(#[source] anyhow::Error),
    #[error("Rate limit exceeded")]
    RateLimitExceeded(#[source] anyhow::Error),
    #[error("Duplicate message detected")]
    DuplicateMessage(#[source] anyhow::Error),
    #[error("Missing idempotency key")] // this is likely unnecessary but idk
    MissingIdempotencyKey(#[source] anyhow::Error),
    #[error(transparent)]
    UnexpectedError(#[from] anyhow::Error),
}

impl ResponseError for MessageError {
    fn status_code(&self) -> StatusCode {
        match self {
            MessageError::InvalidEmail(_) |
            MessageError::MessageLengthError(_) |
            MessageError::NameLengthError(_) |
            MessageError::MissingIdempotencyKey(_) => {
                StatusCode::BAD_REQUEST
            }
            MessageError::RateLimitExceeded(_) => StatusCode::TOO_MANY_REQUESTS,
            MessageError::DuplicateMessage(_) => StatusCode::CONFLICT,
            MessageError::UnexpectedError(_) => StatusCode::INTERNAL_SERVER_ERROR
        }
    }
}

#[derive(serde::Deserialize)]
pub struct MessageForm {
    email: String,
    sender_name: String,
    message_text: String,
}

struct ValidatedMessage {
    email: String,
    sender_name: String,
    message_text: String,
}

impl MessageForm {
    fn validate(&self) -> Result<ValidatedMessage, MessageError> {
        let normalized_email = validate_email(&self.email)
            .map(|email| email.normalized)
            .map_err(|e| MessageError::InvalidEmail(
                anyhow::anyhow!("Invalid email: {:?}", e)
            ))?;

        let trimmed_name = self.validate_name(&self.sender_name)?;
        let trimmed_message = self.validate_message(&self.message_text)?;

        Ok(ValidatedMessage {
            email: normalized_email,
            sender_name: trimmed_name,
            message_text: trimmed_message
        })
    }

    fn validate_name(&self, name: &String) -> Result<String, MessageError> {
        let trimmed_name = name.trim();
        if trimmed_name.len() < 2 || trimmed_name.len() > 100 {
            return Err(MessageError::NameLengthError(
                anyhow::anyhow!("Name must be between 2 an 100 characters.")
            ));
        }

        Ok(trimmed_name.to_string())
    }

    fn validate_message(&self, message: &String) -> Result<String, MessageError> {
        let trimmed_message = message.trim();
        if trimmed_message.len() < 10 || trimmed_message.len() > 5000 {
            return Err(MessageError::MessageLengthError(
                anyhow::anyhow!("Message must be between 10 and 5000 characters.")
            ));
        }

        Ok(trimmed_message.to_string())
    }
}

#[tracing::instrument(
    name = "Send message to contact table",
    skip(form, pool, request),
    fields(email = %form.email)
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
        .ok_or_else(|| MessageError::MissingIdempotencyKey(
            anyhow::anyhow!("Idempotency-Key header is missing")
        ))?
        .to_string()
        .try_into()
        .map_err(|e| {
            tracing::error!("Invalid idempotency key {:?}", e);
            MessageError::MissingIdempotencyKey(anyhow::anyhow!("Invalid idempotency key format: {:?}", e))
        })?;

    let next_action = try_processing(&pool, &idempotency_key, None)
        .await
        .map_err(MessageError::UnexpectedError)?;

    let transaction = match next_action {
        NextAction::StartProcessing(txn) => txn,
        NextAction::ReturnSavedResponse(saved_response)  => {
            tracing::info!("Returning saved response for idempotent request");
            return Ok(saved_response);
        }
    };

    let validated_input = message.0.validate()?;

    let rate_ok = sqlx::query_scalar!(
        "SELECT check_email_rate_limit($1, $2, $3)",
        validated.email,
        3,
        60
    )
    .fetch_one(&pool)
    .await
    .map_err(MessageError::UnexpectedError)?
    .unwrap_or(false);

    if !rate_ok {
        return Err(MessageError::RateLimitExceeded(
            anyhow::anyhow!("Rate limit exceeded")).into()
        );
    }

    let message_id = Uuid::new_v4();
    let result = sqlx::query!(
        r#"
        INSERT INTO messages (message_id, email, sender_name, message_text, created_at)
        VALUES ($1, $2, $3, $4, NOW())
        "#,
        message_id,
        validated_input.email,
        validated_input.sender_name,
        validated_input.message_text,
    )
    .execute(&pool)
    .await;

    match result {
        Ok(_) => {
            tracing::info!("Message saved successfully with: {}", message_id);
            let response = HttpResponse::Accepted().json(serde_json::json!({
                "message": "Message received successfully",
                "message_id": message_id
            }));

            let saved_response = save_response(transaction, &idempotency_key, None, response)
                .await
                .map_err(|e| MessageError::UnexpectedError(e.into()))?;
            
            Ok(saved_response)
        }
        Err(e) => {
            if e.to_string().contains("Duplicate messaged detected") {
                Err(MessageError::DuplicateMessage.into())
            } else {
                tracing::error!("Failed to save message: {:?}", e);
                Err(MessageError::UnexpectedError(e.into()).into())
            }
        }
    }
}

// this seems maybe unnecessary
fn email_validator(email: &String) -> Result<String, ValidationError> {
    validate_email(email)
        .map(|validated| validated.normalized)
        .map_err(|e| {
            tracing::warn!("Email validation failed for '{}': {:?}", email, e);
            e
        })
}