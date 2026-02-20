// start easy, just update published flag
use actix_web::{HttpRequest, HttpResponse, web};
use sqlx::{PgPool, Transaction, Postgres};
use uuid::Uuid;

use crate::{
    authentication::UserId,
    errors::BlogError,
    idempotency::execute_idempotent,
};

#[derive(serde::Deserialize)]
pub struct BlogPatchRequest {
    blog_post_id: Uuid,
    published: bool,
}

#[tracing::instrument(
    name = "Publish blog post",
    skip_all,
)]
pub async fn publish_blog_post(
    blog_patch: web::Json<BlogPatchRequest>,
    user_id: web::ReqData<UserId>,
    request: HttpRequest,
    pool: web::Data<PgPool>
) -> Result<HttpResponse, actix_web::Error> {
    let blog_to_publish = blog_patch.0;
    let user_id = Some(**user_id);

    execute_idempotent(&request, &pool, user_id, move |tx| {
        Box::pin(async move {
            process_patch_blog_post(tx, blog_to_publish).await
        })
    })
    .await
}

#[allow(clippy::future_not_send)]
async fn process_patch_blog_post(
    transaction: &mut Transaction<'static, Postgres>,
    blog_post: BlogPatchRequest
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
    .execute(transaction.as_mut())
    .await
    .map_err(|e| {
        tracing::warn!("Blog post query update failed");
        BlogError::UnexpectedError(anyhow::anyhow!("{e:?}"))
    })?;

    match result.rows_affected() {
        1 => {
            tracing::info!("Post {} updated successfully", post_id);
            Ok(HttpResponse::Accepted().finish())
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