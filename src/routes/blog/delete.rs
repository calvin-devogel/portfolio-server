use actix_web::{HttpRequest, HttpResponse, ResponseError, http::StatusCode, web};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::{
    authentication::UserId,
    idempotency::{
        IdempotencyKey, NextAction, get_idempotency_key, save_response, try_processing
    }
};

#[derive(thiserror::Error, Debug)]
pub enum BlogDeleteError {
    #[error(transparent)]
    UnexpectedError(#[from] anyhow::Error),
    #[error("Post not found")]
    PostNotFound,
}

impl ResponseError for BlogDeleteError {
    fn status_code(&self) -> actix_web::http::StatusCode {
        match self {
            Self::UnexpectedError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::PostNotFound => StatusCode::NOT_FOUND,
        }
    }
}

#[derive(serde::Deserialize)]
pub struct BlogDeleteRequest {
    blog_post_id: Uuid,
}

#[tracing::instrument(
    name = "Delete blog post",
    skip_all,
    fields(user_id = %*user_id, blog_post_id = %blog_delete.blog_post_id)
)]
pub async fn delete_blog_post(
    blog_delete: web::Json<BlogDeleteRequest>,
    user_id: web::ReqData<UserId>,
    request: HttpRequest,
    pool: web::Data<PgPool>
) -> Result<HttpResponse, actix_web::Error> {
    let idempotency_key: IdempotencyKey = get_idempotency_key(request)
        .map_err(|e| {
            tracing::warn!(error = ?e, "Failed to get idempotency key");
            BlogDeleteError::UnexpectedError(anyhow::anyhow!("Failed to get idempotency key: {e:?}"))
        })?;

    let post_to_delete = blog_delete.0;
    let user_id = Some(**user_id);

    let (next_action, transaction) = try_processing(
        &pool, &idempotency_key, user_id
    ).await
    .map_err(|e| {
        tracing::warn!(error = ?e, "Idempotent processing failed");
        BlogDeleteError::UnexpectedError(e.into())
    })?;

    match next_action {
        NextAction::ReturnSavedResponse(saved_response) => {
            tracing::info!("Returning saved response for idempotent request");
            Ok(saved_response)
        }
        NextAction::StartProcessing => {
            let transaction = transaction.expect("Transaction must be present for StartProcessing");
            process_delete_blog_post(
                transaction,
                &pool,
                &idempotency_key,
                post_to_delete,
                user_id
            )
            .await
        }
    }
}

#[allow(clippy::future_not_send)]
async fn process_delete_blog_post(
    transaction: Transaction<'static, Postgres>,
    pool: &PgPool,
    idempotency_key: &IdempotencyKey,
    blog_post: BlogDeleteRequest,
    user_id: Option<Uuid>,
) -> Result<HttpResponse, actix_web::Error> {
    let post_id = blog_post.blog_post_id;

    let result = sqlx::query!(
        r#"
        DELETE FROM blog_posts
        WHERE post_id = $1"#,
        post_id
    )
    .execute(pool)
    .await
    .map_err(|e| {
        tracing::warn!("Blog post delete query failed");
        BlogDeleteError::UnexpectedError(anyhow::anyhow!("{e:?}"))
    })?;

    match result.rows_affected() {
        1 => {
            tracing::info!("Post {} deleted successfully", post_id);
            let response = HttpResponse::Ok().finish();

            let saved_response = save_response(transaction, idempotency_key, user_id, response)
                .await
                .map_err(BlogDeleteError::UnexpectedError)?;

            Ok(saved_response)
        }
        0 => {
            tracing::warn!("Blog post not found: {}", post_id);
            Err(BlogDeleteError::PostNotFound.into())
        }
        rows => {
            tracing::error!(
                "Unexpected rows affected: {} for blog_post_id: {}",
                rows,
                post_id
            );
            Err(BlogDeleteError::UnexpectedError(anyhow::anyhow!(
                "Unexpected rows affected: {}",
                rows
            ))
            .into())
        }
    }
}