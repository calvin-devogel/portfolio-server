// src/modules/contact/db.rs
use sqlx::{PgExecutor, PgPool};
use uuid::Uuid;

use super::models::MessageRecord;

pub async fn count_messages(pool: &PgPool) -> Result<i64, sqlx::Error> {
    let total = sqlx::query_scalar!("SELECT COUNT(*) FROM messages")
        .fetch_one(pool)
        .await?;

    Ok(total.unwrap_or(0))
}

pub async fn fetch_messages(
    pool: &PgPool,
    limit: i64,
    offset: i64,
) -> Result<Vec<MessageRecord>, sqlx::Error> {
    sqlx::query_as!(
        MessageRecord,
        r#"
        SELECT message_id, email, sender_name, message_text, created_at, read_message
        FROM messages
        ORDER BY created_at DESC
        LIMIT $1 OFFSET $2
        "#,
        limit,
        offset
    )
    .fetch_all(pool)
    .await
}

pub async fn update_message_read_status<'a>(
    executor: impl PgExecutor<'a>,
    message_id: Uuid,
    is_read: bool,
) -> Result<u64, sqlx::Error> {
    let result = sqlx::query!(
        r#"
        UPDATE messages
        SET read_message = $2
        WHERE message_id = $1
        "#,
        message_id,
        is_read
    )
    .execute(executor)
    .await?;

    Ok(result.rows_affected())
}

pub async fn check_email_rate_limit<'a>(
    executor: impl PgExecutor<'a>,
    email: &str,
    max_messages: i32,
    window_minutes: i32,
) -> Result<bool, sqlx::Error> {
    let rate_ok = sqlx::query_scalar!(
        "SELECT check_email_rate_limit($1, $2, $3)",
        email,
        max_messages,
        window_minutes
    )
    .fetch_one(executor)
    .await?;

    Ok(rate_ok.unwrap_or(false))
}

pub async fn insert_message<'a>(
    executor: impl PgExecutor<'a>,
    message_id: Uuid,
    email: String,
    sender_name: String,
    message_text: String,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        r#"
        INSERT INTO messages(message_id, email, sender_name, message_text, created_at, read_message)
        VALUES ($1, $2, $3, $4, NOW(), FALSE)
        "#,
        message_id,
        email,
        sender_name,
        message_text
    )
    .execute(executor)
    .await?;

    Ok(())
}
