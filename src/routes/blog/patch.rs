// start easy, just update published flag
use actix_web::{HttpRequest, HttpResponse, web};
use sqlx::{
    PgPool,
    Postgres,
    Transaction,
    QueryBuilder,
};
use uuid::Uuid;

use crate::{authentication::UserId, errors::BlogError, idempotency::execute_idempotent};

#[derive(serde::Deserialize)]
pub struct BlogPatchRequest {
    post_id: Uuid,
    published: bool,
}

#[derive(serde::Deserialize)]
pub struct BlogEditRequest {
    post_id: Uuid,
    title: Option<String>,
    content: Option<String>,
    excerpt: Option<String>,
    author: Option<String>,
}

#[tracing::instrument(name = "Edit blog post", skip_all)]
pub async fn edit_blog_post(
    blog_edit_request: web::Json<BlogEditRequest>,
    user_id: web::ReqData<UserId>,
    request: HttpRequest,
    pool: web::Data<PgPool>,
) -> Result<HttpResponse, actix_web::Error> {
    let blog_to_edit = blog_edit_request.into_inner();
    let user_id = Some(*user_id.into_inner());

    execute_idempotent(&request, &pool, user_id, move |tx| {
        Box::pin(async move { process_edit_blog_post(tx, blog_to_edit).await })
    })
    .await
}

#[allow(clippy::future_not_send)]
async fn process_edit_blog_post(
    transaction: &mut Transaction<'static, Postgres>,
    blog_post: BlogEditRequest,
) -> Result<HttpResponse, actix_web::Error> {
    let post_id = blog_post.post_id;

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

    push_if_some!(blog_post.title, "title");
    push_if_some!(blog_post.content, "content");
    push_if_some!(blog_post.excerpt, "excerpt");
    push_if_some!(blog_post.author, "author");

    builder.push(", updated_at = NOW() WHERE post_id = ");
    builder.push_bind(post_id);

    if builder.sql().contains(r#"UPDATE blog_posts SET , updated_at = NOW() WHERE post_id = "#) {
        tracing::warn!("No fields to update for post {}", post_id);
        return Err(BlogError::UnexpectedError(anyhow::anyhow!("No fields provided to update")).into())
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
pub async fn publish_blog_post(
    blog_patch: web::Json<BlogPatchRequest>,
    user_id: web::ReqData<UserId>,
    request: HttpRequest,
    pool: web::Data<PgPool>,
) -> Result<HttpResponse, actix_web::Error> {
    let blog_to_publish = blog_patch.0;
    let user_id = Some(*user_id.into_inner());

    execute_idempotent(&request, &pool, user_id, move |tx| {
        Box::pin(async move { process_publish_blog_post(tx, blog_to_publish).await })
    })
    .await
}

#[allow(clippy::future_not_send)]
async fn process_publish_blog_post(
    transaction: &mut Transaction<'static, Postgres>,
    blog_post: BlogPatchRequest,
) -> Result<HttpResponse, actix_web::Error> {
    let post_id = blog_post.post_id;
    let is_published = blog_post.published;

    let result = sqlx::query!(
        r#"
        UPDATE blog_posts
        SET published = $2, updated_at = NOW()
        WHERE post_id = $1"#,
        blog_post.post_id,
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
