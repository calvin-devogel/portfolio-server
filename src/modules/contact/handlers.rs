use actix_web::{HttpRequest, HttpResponse, web};
use chrono::{DateTime, Utc};
use uuid::Uuid;

use email_address::EmailAddress;
use std::ops::Deref;
use std::str::FromStr;

use crate::core::MessageRateLimitSettings;
use crate::errors::ContactSubmissionError;

use crate::{
    core::{PaginationMeta, PaginationQuery},
    errors::MessageGetError,
};

use sqlx::{PgPool, Postgres, Transaction};

use crate::{
    api::idempotency::execute_idempotent, errors::MessagePatchError, modules::auth::UserId,
};

// query messages in page form, minimum 0, maximum 20 per page
// on read, should set the message_read column to TRUE
// admin should be able to delete, highlight (star) messages
// does this need any other functionality?

#[derive(serde::Serialize)]
struct MessageRecord {
    message_id: Uuid,
    email: String,
    sender_name: String,
    message_text: String,
    created_at: DateTime<Utc>,
    read_message: Option<bool>,
}

#[derive(serde::Serialize)]
struct MessagesResponse {
    // Keep your old top-level list key:
    messages: Vec<MessageRecord>, // <- use your existing message DTO type

    // Keep old pagination keys:
    page: i64,
    page_size: i64,
    total_items: i64,
    total_pages: i64,
}

#[tracing::instrument(name = "Get messages with pagination", skip(pool))]
pub async fn get_messages(
    query: web::Query<PaginationQuery>,
    pool: web::Data<PgPool>,
) -> Result<HttpResponse, actix_web::Error> {
    let q = query.into_inner();
    let page_size = q.page_size();
    let offset = q.offset();
    // total count
    let total_count = sqlx::query_scalar!("SELECT COUNT(*) FROM messages")
        .fetch_one(pool.as_ref())
        .await
        .map_err(|e| {
            tracing::error!("Failed to get message count: {e:?}");
            MessageGetError::TotalCount
        })?
        .unwrap_or(0);

    let messages = sqlx::query_as!(
        MessageRecord,
        r#"
        SELECT message_id, email, sender_name, message_text, created_at, read_message
        FROM messages
        ORDER BY created_at DESC
        LIMIT $1 OFFSET $2"#,
        page_size,
        offset
    )
    .fetch_all(pool.as_ref())
    .await
    .map_err(|e| {
        tracing::error!("Failed to fetch messages: {e:?}");
        actix_web::error::ErrorInternalServerError("Failed to retrieve messages")
    })?;

    let meta = PaginationMeta::from_total(total_count, &q);

    let response = MessagesResponse {
        messages,
        page: meta.page,
        page_size: meta.page_size,
        total_items: meta.total_items,
        total_pages: meta.total_pages,
    };

    Ok(HttpResponse::Ok().json(response))
}

#[derive(serde::Deserialize)]
pub struct MessagePatchRequest {
    message_id: Uuid,
    read: bool,
}

#[tracing::instrument(
    name = "Update message",
    skip_all,
    fields(user_id = %*user_id, message_id = %message.message_id)
)]
pub async fn patch_message(
    message: web::Json<MessagePatchRequest>,
    user_id: UserId,
    request: HttpRequest,
    pool: web::Data<PgPool>,
) -> Result<HttpResponse, actix_web::Error> {
    let message_to_patch = message.0;

    execute_idempotent(&request, &pool, Some(*user_id), move |tx| {
        Box::pin(async move { process_patch_message(tx, message_to_patch).await })
    })
    .await
}

#[allow(clippy::future_not_send)]
async fn process_patch_message(
    transaction: &mut Transaction<'static, Postgres>,
    message: MessagePatchRequest,
) -> Result<HttpResponse, actix_web::Error> {
    let message_id = message.message_id;
    let is_read = message.read;

    let result = sqlx::query!(
        r#"
        UPDATE messages
        SET read_message = $2
        WHERE message_id = $1
        "#,
        message_id,
        is_read
    )
    .execute(transaction.as_mut())
    .await
    .map_err(|e| {
        tracing::warn!("Message update query failed");
        MessagePatchError::UnexpectedError(anyhow::anyhow!("Message update query failed: {e:?}"))
    })?;

    match result.rows_affected() {
        1 => {
            tracing::info!("Message {} updated successfully", message_id);
            Ok(HttpResponse::Accepted().finish())
        }
        0 => {
            tracing::warn!("Message not found: {}", message_id);
            Err(MessagePatchError::MessageNotFound.into())
        }
        rows => {
            tracing::error!(
                "Unexpected rows affected: {} for message_id: {}",
                rows,
                message_id
            );
            Err(MessagePatchError::UnexpectedError(anyhow::anyhow!(
                "Unexpected rows affected: {rows}"
            ))
            .into())
        }
    }
}

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
    let config_for_op = message_config.clone();

    execute_idempotent(&request, pool.get_ref(), None, move |tx| {
        let config_for_op = config_for_op.clone();
        Box::pin(
            async move { process_new_message(tx, config_for_op.get_ref(), message_to_post).await },
        )
    })
    .await
}

#[allow(clippy::future_not_send)]
// consume the transaction immediately for Send safety
async fn process_new_message(
    transaction: &mut Transaction<'static, Postgres>,
    config: &MessageRateLimitSettings,
    message: MessageForm,
) -> Result<HttpResponse, actix_web::Error> {
    let validated_input = message.validate()?;

    let rate_ok = sqlx::query_scalar!(
        "SELECT check_email_rate_limit($1, $2, $3)",
        &validated_input.email,
        i32::try_from(config.max_messages).expect("Failed to cast config.max_messages"),
        i32::try_from(config.window_minutes).expect("Failed to cast config.window_minutes")
    )
    .fetch_one(transaction.as_mut())
    .await
    .map_err(|e| {
        ContactSubmissionError::UnexpectedError(anyhow::anyhow!("Unexpected error: {e:?}"))
    })?
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
    .execute(transaction.as_mut())
    .await;

    match result {
        Ok(_) => {
            tracing::info!("Message saved successfully with: {}", message_id);
            Ok(HttpResponse::Accepted().json(MessageResponse::new(
                "Message received successfully",
                message_id,
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
