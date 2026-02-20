// start easy, just update published flag
use actix_web::{HttpRequest, HttpResponse, web};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::{
    authentication::UserId,
    errors::BlogError,
    idempotency::{IdempotencyKey, NextAction, get_idempotency_key, save_response, try_processing}
};

#[derive(serde::Deserialize)]
pub struct BlogPatchRequest {
    blog_post_id: Uuid,
    published: bool,
}

#[tracing::instrument(
    name = "Publish blog post",
    skip_all,
    fields(user_id = %*user_id, blog_post_id = %blog_patch.blog_post_id)
)]
pub async fn publish_blog_post(
    blog_patch: web::Json<BlogPatchRequest>,
    user_id: web::ReqData<UserId>,
    request: HttpRequest,
    pool: web::Data<PgPool>
) -> Result<HttpResponse, actix_web::Error> {
    let idempotency_key: IdempotencyKey = get_idempotency_key(request)
        .map_err(|e| {
            tracing::warn!(error = ?e, "Failed to get idempotency key");
            BlogError::UnexpectedError(anyhow::anyhow!("Failed to get idempotency key: {e:?}"))
        })?;

    let post_to_publish = blog_patch.0;
    let user_id = Some(**user_id);

    let (next_action, transaction) = try_processing(
        &pool, &idempotency_key, user_id
    ).await
    .map_err(|e| {
        tracing::warn!(error = ?e, "Idempotent processing failed");
        BlogError::UnexpectedError(e.into())
    })?;

    match next_action {
        NextAction::ReturnSavedResponse(saved_response) => {
            tracing::info!("Returning saved response for idempotent request");
            Ok(saved_response)
        }
        NextAction::StartProcessing => {
            let transaction = transaction.expect("Transaction must be present for StartProcessing");
            process_patch_blog_post(
                transaction,
                &pool,
                &idempotency_key,
                post_to_publish,
                user_id
            )
            .await
        }
    }
}

#[allow(clippy::future_not_send)]
async fn process_patch_blog_post(
    transaction: Transaction<'static, Postgres>,
    pool: &PgPool,
    idempotency_key: &IdempotencyKey,
    blog_post: BlogPatchRequest,
    user_id: Option<Uuid>,
) -> Result<HttpResponse, actix_web::Error> {
    let post_id = blog_post.blog_post_id;
    let is_published = blog_post.published;

    let result = sqlx::query!(
        r#"
        UPDATE blog_posts
        SET published = $2
        WHERE post_id = $1"#,
        blog_post.blog_post_id,
        is_published
    )
    .execute(pool)
    .await
    .map_err(|e| {
        tracing::warn!("Blog post query update failed");
        BlogError::UnexpectedError(anyhow::anyhow!("{e:?}"))
    })?;

    match result.rows_affected() {
        1 => {
            tracing::info!("Post {} updated successfully", post_id);
            let response = HttpResponse::Accepted().finish();

            let saved_response = save_response(transaction, idempotency_key, user_id, response)
                .await
                .map_err(BlogError::UnexpectedError)?;

            Ok(saved_response)
        }
        0 => {
            tracing::warn!("Blog post not found: {}", post_id);
            Err(BlogError::PostNotFound.into())
        }
        rows => {
            tracing::error!(
                "Unexpected rows affected: {} for blog_post_id: {}",
                rows,
                post_id
            );
            Err(BlogError::UnexpectedError(anyhow::anyhow!(
                "Unexpected rows affected: {}",
                rows
            ))
            .into())
        }
    }
}