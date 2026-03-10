// start easy, just update published flag
use actix_web::{HttpRequest, HttpResponse, web};
use sqlx::{PgPool, Postgres, QueryBuilder, Transaction};

use crate::{
    authentication::UserId,
    // ArticleError?
    errors::BlogError,
    idempotency::execute_idempotent,
    types::article::{ArticleEditRequest, ArticlePublishRequest},
};

#[tracing::instrument(name = "Edit blog post", skip_all)]
pub async fn edit_article(
    article_edit_request: web::Json<ArticleEditRequest>,
    user_id: web::ReqData<UserId>,
    request: HttpRequest,
    pool: web::Data<PgPool>,
) -> Result<HttpResponse, actix_web::Error> {
    let article_to_edit = article_edit_request.into_inner();
    let user_id = Some(*user_id.into_inner());

    article_to_edit.validate().map_err(actix_web::Error::from)?;

    execute_idempotent(&request, &pool, user_id, move |tx| {
        Box::pin(async move { process_edit_article(tx, article_to_edit).await })
    })
    .await
}

#[allow(clippy::future_not_send)]
async fn process_edit_article(
    transaction: &mut Transaction<'static, Postgres>,
    article: ArticleEditRequest,
) -> Result<HttpResponse, actix_web::Error> {
    let post_id = article.post_id;

    let mut builder = QueryBuilder::<Postgres>::new("UPDATE blog_posts SET ");
    let mut separator = builder.separated(", ");

    // macros!
    macro_rules! push_if_some {
        ($field:expr, $col:literal) => {
            if let Some(val) = $field {
                separator.push(concat!($col, "= "));
                separator.push_bind_unseparated(val);
            }
        };
    }

    push_if_some!(article.title, "title");
    push_if_some!(article.excerpt, "excerpt");
    push_if_some!(article.author, "author");

    if let Some(sections) = article.sections {
        let sections_json = serde_json::to_value(&sections)
            .map_err(|e| BlogError::UnexpectedError(anyhow::anyhow!(e)))?;
        separator.push("sections = ");
        separator.push_bind_unseparated(sections_json);
    }

    builder.push(", updated_at = NOW() WHERE post_id = ");
    builder.push_bind(post_id);

    if builder
        .sql()
        .contains("UPDATE blog_posts SET , updated_at = NOW() WHERE post_id = ")
    {
        tracing::warn!("No fields to update for post {}", post_id);
        return Err(
            BlogError::UnexpectedError(anyhow::anyhow!("No fields provided to update")).into(),
        );
    }

    let result = builder
        .build()
        .execute(transaction.as_mut())
        .await
        .map_err(|e| {
            tracing::warn!("Blog post update query failed");
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
            Err(
                BlogError::UnexpectedError(anyhow::anyhow!("Unexpected rows affected: {rows}"))
                    .into(),
            )
        }
    }
}

#[tracing::instrument(name = "Publish blog post", skip_all)]
pub async fn publish_article(
    article: web::Json<ArticlePublishRequest>,
    user_id: web::ReqData<UserId>,
    request: HttpRequest,
    pool: web::Data<PgPool>,
) -> Result<HttpResponse, actix_web::Error> {
    let article_to_publish = article.0;
    let user_id = Some(*user_id.into_inner());

    execute_idempotent(&request, &pool, user_id, move |tx| {
        Box::pin(async move { process_publish_article(tx, article_to_publish).await })
    })
    .await
}

#[allow(clippy::future_not_send)]
async fn process_publish_article(
    transaction: &mut Transaction<'static, Postgres>,
    article: ArticlePublishRequest,
) -> Result<HttpResponse, actix_web::Error> {
    let post_id = article.post_id;
    let is_published = article.published;

    let result = sqlx::query!(
        r#"
        UPDATE blog_posts
        SET published = $2, updated_at = NOW()
        WHERE post_id = $1"#,
        article.post_id,
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
            Err(
                BlogError::UnexpectedError(anyhow::anyhow!("Unexpected rows affected: {rows}"))
                    .into(),
            )
        }
    }
}
