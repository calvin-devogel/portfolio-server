use actix_web::{HttpResponse, web};
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

// query messages in page form, minimum 0, maximum 20 per page
// on read, should set the message_read column to TRUE
// admin should be able to delete, highlight (star) messages
// does this need any other functionality?

// tragically, psql returns i64 for int sizes, can't shrink these until
// after the query
#[derive(serde::Deserialize, Debug)]
pub struct MessageQuery {
    #[serde(default)]
    page: i64,
    #[serde(default = "default_page_size")]
    page_size: i64,
}

const fn default_page_size() -> i64 {
    20
}

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
    messages: Vec<MessageRecord>,
    page: i64,
    page_size: i64,
    total_count: i64,
}

#[tracing::instrument(
    name = "Get messages with pagination",
    skip(pool),
    fields(page = %query.page, page_size = %query.page_size)
)]
pub async fn get_messages(
    query: web::Query<MessageQuery>,
    pool: web::Data<PgPool>,
) -> Result<HttpResponse, actix_web::Error> {
    // validate pagination
    let page = query.page.max(0);
    let page_size = query.page_size.clamp(1, 20);
    let offset = page * page_size;

    // total count
    let total_count = sqlx::query_scalar!("SELECT COUNT(*) FROM messages")
        .fetch_one(pool.as_ref())
        .await
        .map_err(|e| {
            tracing::error!("Failed to get message count: {e:?}");
            actix_web::error::ErrorInternalServerError("Failed to retrieve message count")
        })? // come back to this just get it written
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

    tracing::info!(
        "Retrieved {} messages for page {} (page_size: {})",
        messages.len(),
        page,
        page_size
    );

    Ok(HttpResponse::Ok().json(MessagesResponse {
        messages,
        page,
        page_size,
        total_count,
    }))
}
