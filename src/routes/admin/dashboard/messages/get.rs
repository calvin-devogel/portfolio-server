use actix_web::{HttpResponse, web};
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::{
    errors::MessageGetError,
    pagination::{PaginationMeta, PaginationQuery}
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

#[tracing::instrument(
    name = "Get messages with pagination",
    skip(pool),
)]
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
        total_pages: meta.total_pages
    };

    Ok(HttpResponse::Ok().json(response))
}
