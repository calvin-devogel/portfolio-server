use actix_web::{HttpResponse, HttpRequest, web};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::{
    authentication::UserId,
    errors::MessagePatchError, 
    idempotency::{
        IdempotencyKey, NextAction, save_response, try_processing, get_idempotency_key}
};

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
    let idempotency_key: IdempotencyKey = get_idempotency_key(request)
        .map_err(|e| {
            tracing::warn!(error = ?e, "Failed to get idempotency key");
            MessagePatchError::UnexpectedError(anyhow::anyhow!("Failed to get idempotency key"))
        })?;
    let message_to_patch = message.0;
    let user_id = Some(**user_id);

    let (next_action, transaction) = try_processing(&pool, &idempotency_key, user_id)
        .await
        .map_err(|e| {
            tracing::warn!(error = ?e, "Idempotent processing failed");
            MessagePatchError::UnexpectedError(anyhow::anyhow!("Idempotent processing failed"))
        })?;

    match next_action {
        NextAction::ReturnSavedResponse(saved_response) => {
            tracing::info!("Returning saved response for idempotent request");
            Ok(saved_response)
        }
        NextAction::StartProcessing => {
            let transaction = transaction.expect("Transaction must be present for StartProcessing");
            process_patch_message(
                transaction,
                &pool,
                &idempotency_key,
                message_to_patch,
                user_id
            )
            .await
        }
    }
}

#[allow(clippy::future_not_send)]
async fn process_patch_message(
    transaction: Transaction<'static, Postgres>,
    pool: &PgPool,
    idempotency_key: &IdempotencyKey,
    message: MessagePatchRequest,
    user_id: Option<Uuid>
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
    .execute(pool)
    .await
    .map_err(|e| {
        tracing::warn!("Message update query failed");
        MessagePatchError::UnexpectedError(anyhow::anyhow!("Message update query failed: {e:?}"))
    })?;

    match result.rows_affected() {
        1 => {
            tracing::info!("Message {} updated successfully", message_id);
            let response = HttpResponse::Accepted().finish();

            let saved_response = save_response(transaction, idempotency_key, user_id, response)
                .await
                .map_err(MessagePatchError::UnexpectedError)?;

            Ok(saved_response)
        }
        0 => {
            tracing::warn!("Message not found: {}", message_id);
            Err(MessagePatchError::MessageNotFound.into())
        }
        rows => {
            tracing::error!("Unexpected rows affected: {} for message_id: {}", rows, message_id);
            Err(MessagePatchError::UnexpectedError(
                anyhow::anyhow!("Unexpected rows affected: {}", rows)
            ).into())
        }
    }
}