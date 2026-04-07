// src/modules/contact/handlers.rs
use actix_web::{HttpRequest, HttpResponse, web};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::{
    api::idempotency::execute_idempotent,
    core::{MessageRateLimitSettings, PaginationMeta, PaginationQuery},
    core::error::Contact,
    modules::auth::UserId,
};

// Import DB logic and Data models
use super::db::{
    check_email_rate_limit, count_messages, fetch_messages, insert_message,
    update_message_read_status,
};
use super::models::{
    MessageForm, MessageId, MessagePatchRequest, MessageResponse, MessagesResponse,
};

#[tracing::instrument(name = "Get messages with pagination", skip(pool))]
pub async fn get_messages(
    query: web::Query<PaginationQuery>,
    pool: web::Data<PgPool>,
) -> Result<HttpResponse, actix_web::Error> {
    let q = query.into_inner();
    let page_size = q.page_size();
    let offset = q.offset();

    let total_count = count_messages(pool.as_ref()).await.map_err(|e| {
        Contact::Unexpected(anyhow::anyhow!("Failed to get message count: {e:?}"))
    })?;

    let messages = fetch_messages(pool.as_ref(), page_size, offset)
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

    let rows = update_message_read_status(transaction.as_mut(), message_id, message.read)
        .await
        .map_err(|e| {
            tracing::warn!("Message update query failed");
            Contact::Unexpected(anyhow::anyhow!(
                "Message update query failed: {e:?}"
            ))
        })?;

    match rows {
        1 => {
            tracing::info!("Message {} updated successfully", message_id);
            Ok(HttpResponse::Accepted().finish())
        }
        0 => {
            tracing::warn!("Message not found: {}", message_id);
            Err(Contact::NotFound(message_id).into())
        }
        rows => {
            tracing::error!(
                "Unexpected rows affected: {} for message_id: {}",
                rows,
                message_id
            );
            Err(Contact::Unexpected(anyhow::anyhow!(
                "Unexpected rows affected: {rows}"
            ))
            .into())
        }
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
async fn process_new_message(
    transaction: &mut Transaction<'static, Postgres>,
    config: &MessageRateLimitSettings,
    message: MessageForm,
) -> Result<HttpResponse, actix_web::Error> {
    let validated_input = message.validate()?;

    let max_msg = i32::try_from(config.max_messages).expect("Failed to cast config.max_messages");
    let win_min =
        i32::try_from(config.window_minutes).expect("Failed to cast config.window_minutes");

    let rate_ok = check_email_rate_limit(
        transaction.as_mut(),
        &validated_input.email,
        max_msg,
        win_min,
    )
    .await
    .map_err(|e| {
        Contact::Unexpected(anyhow::anyhow!("Unexpected error: {e:?}"))
    })?;

    if !rate_ok {
        return Err(Contact::RateLimited.into());
    }

    let message_id = MessageId(Uuid::new_v4());
    tracing::Span::current().record("message_id", tracing::field::display(&message_id));

    let result = insert_message(
        transaction.as_mut(),
        *message_id,
        validated_input.email,
        validated_input.sender_name,
        validated_input.message_text,
    )
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
                Err(Contact::Duplicate.into())
            } else {
                tracing::error!("Failed to save message: {e:?}");
                Err(Contact::Unexpected(e.into()).into())
            }
        }
    }
}
