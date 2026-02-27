use actix_web::{HttpRequest, HttpResponse, web};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::{authentication::UserId, errors::MessagePatchError, idempotency::execute_idempotent};

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
    user_id: web::ReqData<UserId>,
    request: HttpRequest,
    pool: web::Data<PgPool>,
) -> Result<HttpResponse, actix_web::Error> {
    let message_to_patch = message.0;
    let user_id = Some(**user_id);

    execute_idempotent(&request, &pool, user_id, move |tx| {
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
