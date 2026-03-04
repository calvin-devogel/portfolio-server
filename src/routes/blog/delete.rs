use actix_web::{HttpRequest, HttpResponse, web};
use sqlx::{PgPool, Postgres, Transaction};

use crate::{
    authentication::UserId,
    errors::BlogError,
    idempotency::execute_idempotent,
    types::article::ArticleDeleteRequest,
};

#[tracing::instrument(
    name = "Delete blog post",
    skip_all,
    fields(user_id = %*user_id, article_id = %article.post_id)
)]
pub async fn delete_article(
    article: web::Json<ArticleDeleteRequest>,
    user_id: web::ReqData<UserId>,
    request: HttpRequest,
    pool: web::Data<PgPool>,
) -> Result<HttpResponse, actix_web::Error> {
    let article_to_delete = article.0;
    let user_id = Some(**user_id);

    execute_idempotent(&request, &pool, user_id, move |tx| {
        Box::pin(async move { process_delete_article(tx, article_to_delete).await })
    })
    .await
}

#[allow(clippy::future_not_send)]
async fn process_delete_article(
    transaction: &mut Transaction<'static, Postgres>,
    article: ArticleDeleteRequest,
) -> Result<HttpResponse, actix_web::Error> {
    let post_id = article.post_id;

    let result = sqlx::query!(
        r#"
        DELETE FROM blog_posts
        WHERE post_id = $1
        "#,
        post_id
    )
    .execute(transaction.as_mut())
    .await
    .map_err(|e| {
        tracing::warn!("Blog post delete query failed");
        BlogError::UnexpectedError(anyhow::anyhow!("{e:?}"))
    })?;

    match result.rows_affected() {
        1 => {
            tracing::info!("Post {} deleted successfully", post_id);
            Ok(HttpResponse::Ok().finish())
        }
        0 => {
            tracing::warn!("Blog post not found: {}", post_id);
            Err(BlogError::PostNotFound.into())
        }
        rows => {
            tracing::error!(
                "Unexpected rows affected: {} for post id: {}",
                rows,
                post_id
            );
            Err(
                BlogError::UnexpectedError(anyhow::anyhow!("Unexpected rows affected: {rows}"))
                    .into(),
            )
        }
    }
}
