use actix_web::{HttpResponse, HttpRequest, http::StatusCode, ResponseError, web};
use email_address::EmailAddress;
use sqlx::{PgPool, Postgres, Transaction};
use std::str::FromStr;
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
            Self::InvalidEmail(_) |
            Self::MessageLengthError(_) |
            Self::NameLengthError(_) |
            Self::MissingIdempotencyKey(_) => {
                StatusCode::BAD_REQUEST
            }
            Self::RateLimitExceeded(_) => StatusCode::TOO_MANY_REQUESTS,
            Self::DuplicateMessage(_) => StatusCode::CONFLICT,
            Self::UnexpectedError(_) => StatusCode::INTERNAL_SERVER_ERROR
        }
    }
}

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
    fn validate(&self) -> Result<ValidatedMessage, MessageError> {
        // emval is bloated and we don't need it
        // use email_address instead
        // let normalized_email = validate_email(&self.email)
        //     .map(|email| email.normalized)
        //     .map_err(|e| MessageError::InvalidEmail(
        //         anyhow::anyhow!("Invalid email: {e:?}")
        //     ))?;
        let validated_email = EmailAddress::from_str(&self.email)
            .map(|r| r.email().to_string())
            .map_err(|e| MessageError::InvalidEmail(
                anyhow::anyhow!("Invalid email: {e:?}")
            ))?;

        
        let trimmed_name = self.validate_name()?;
        let trimmed_message = self.validate_message()?;

        Ok(ValidatedMessage {
            email: validated_email,
            sender_name: trimmed_name,
            message_text: trimmed_message
        })
    }

    fn validate_name(&self) -> Result<String, MessageError> {
        let trimmed_name = self.sender_name.trim();
        if trimmed_name.len() < 2 || trimmed_name.len() > 100 {
            return Err(MessageError::NameLengthError(
                anyhow::anyhow!("Name must be between 2 an 100 characters.")
            ));
        }

        Ok(trimmed_name.to_string())
    }

    fn validate_message(&self) -> Result<String, MessageError> {
        let trimmed_message = self.message_text.trim();
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
        .ok_or_else(|| MessageError::MissingIdempotencyKey(
            anyhow::anyhow!("Idempotency-Key header is missing")
        ))?
        .to_string()
        .try_into()
        .map_err(|e| {
            tracing::error!("Invalid idempotency key {:?}", e);
            MessageError::MissingIdempotencyKey(anyhow::anyhow!("Invalid idempotency key format: {e:?}"))
        })?;
    
    let validated_input = message.0.validate()?;

    let next_action = try_processing(&pool, &idempotency_key, None)
        .await
        .map_err(MessageError::UnexpectedError)?;

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
    .map_err(|e| MessageError::UnexpectedError(
        anyhow::anyhow!("Unexpected error: {e:?}")
    ))?
    .unwrap_or(false);

    if !rate_ok {
        return Err(MessageError::RateLimitExceeded(
            anyhow::anyhow!("Rate limit exceeded")).into()
        );
    }

    let message_id = Uuid::new_v4();
    let result = sqlx::query!(
        r#"
        INSERT INTO messages(message_id, email, sender_name, message_text, created_at)
        VALUES ($1, $2, $3, $4, NOW())
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
                .map_err(MessageError::UnexpectedError)?;
            
            Ok(saved_response)
        }
        Err(e) => {
            if e.to_string().contains("Duplicate message detected") {
                Err(MessageError::DuplicateMessage(anyhow::anyhow!("Duplicate message detected")).into())
            } else {
                tracing::error!("Failed to save message: {e:?}");
                Err(MessageError::UnexpectedError(e.into()).into())
            }
        }
    }
}