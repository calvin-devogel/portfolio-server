use actix_web::{HttpRequest, HttpResponse, ResponseError, http::StatusCode, web};
use sqlx::{PgPool, Postgres, Transaction};
use std::ops::Deref;
use uuid::Uuid;

use crate::{idempotency::{
    IdempotencyKey, NextAction, get_idempotency_key, save_response, try_processing,
}};

#[derive(serde::Deserialize)]
pub struct BlogPostForm {
    title: String,
    content: String,
    excerpt: String,
    author: String,
    user_id: Uuid,
}

#[derive(Clone, Copy, Debug, serde::Serialize)]
pub struct BlogPostId(Uuid);

impl std::fmt::Display for BlogPostId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl Deref for BlogPostId {
    type Target = Uuid;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(serde::Serialize)]
struct BlogPostResponse {
    message: &'static str,
    post_id: BlogPostId,
}

impl BlogPostResponse {
    pub const fn new(message: &'static str, post_id: BlogPostId) -> Self {
        Self {
            message,
            post_id
        }
    }
}

#[derive(thiserror::Error, Debug)]
pub enum BlogPostError {
    #[error(transparent)]
    UnexpectedError(#[from] anyhow::Error),
    #[error("Duplicate post")]
    DuplicatePost
}

impl ResponseError for BlogPostError {
    fn status_code(&self) -> actix_web::http::StatusCode {
        match self {
            Self::UnexpectedError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::DuplicatePost => StatusCode::CONFLICT,
        }
    }
}

#[tracing::instrument(
    name = "Insert blog post",
    skip(blog_post, pool, request),
    fields(
        post_id = tracing::field::Empty
    )
)]
pub async fn insert_blog_post(
    blog_post: web::Form<BlogPostForm>,
    pool: web::Data<PgPool>,
    request: HttpRequest,
) -> Result<HttpResponse, actix_web::Error> {
    let idempotency_key: IdempotencyKey = get_idempotency_key(request)
        .map_err(|e| {
            tracing::warn!(error = ?e, "Failed to get idempotency key");
            BlogPostError::UnexpectedError(anyhow::anyhow!("Failed to get idempotency key: {e:?}"))
        })?;

    let (next_action, transaction) = try_processing(&pool, &idempotency_key, Some(blog_post.user_id))
        .await
        .map_err(|e| {
            tracing::warn!(error = ?e, "Idempotent processing failed");
            BlogPostError::UnexpectedError(anyhow::anyhow!("Idempotent processing failed: {e:?}"))
        })?;
    
    match next_action {
        NextAction::ReturnSavedResponse(saved_response) => {
            tracing::info!("Returning saved response for idempotent request");
            Ok(saved_response)
        }
        NextAction::StartProcessing => {
            let transaction = transaction.expect("Transaction must be present for StartProcessing");
            process_new_blog_post(
                transaction,
                &idempotency_key,
                blog_post.0,
            )
            .await
        }
    }
}

#[allow(clippy::future_not_send)]
async fn process_new_blog_post(
    mut transaction: Transaction<'static, Postgres>,
    idempotency_key: &IdempotencyKey,
    blog_post: BlogPostForm
) -> Result<HttpResponse, actix_web::Error> {
    let post_id = BlogPostId(Uuid::new_v4());
    let slug = get_blog_post_slug(&blog_post.title);
    tracing::Span::current().record("post_id", tracing::field::display(&post_id));

    let result = sqlx::query!(
        r#"
        INSERT INTO blog_posts(
        post_id,
        title,
        slug,
        content,
        excerpt,
        author,
        published,
        created_at,
        updated_at)
        VALUES ($1, $2, $3, $4, $5, $6, FALSE, NOW(), NOW())"#,
        *post_id,
        blog_post.title,
        slug,
        blog_post.content,
        blog_post.excerpt,
        blog_post.author
    )
    .execute(&mut *transaction)
    .await;


    match result {
        Ok(_) => {
            tracing::info!("Post saved successfully with: {}", post_id);
            let response = HttpResponse::Accepted().json(BlogPostResponse::new(
                "Post received successfully",
                post_id
            ));

            let saved_response = save_response(transaction, idempotency_key, Some(blog_post.user_id), response)
                .await
                .map_err(BlogPostError::UnexpectedError)?;
            
            Ok(saved_response)
        }
        Err(e) => {
            if e.to_string().contains("Duplicate message detected") {
                tracing::warn!("Duplicate message detected");
                Err(BlogPostError::DuplicatePost.into())
            } else {
                tracing::error!("Failed to save message: {e:?}");
                Err(BlogPostError::UnexpectedError(e.into()).into())
            }
        }
    }
}

fn get_blog_post_slug(title: &String) -> String {
    str::replace(&title.clone(), " ", "-").to_ascii_lowercase()
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn blog_post_slug() {
        let title = "New Blog Title".to_string();
        let slug = get_blog_post_slug(&title);
        assert_eq!(slug, "new-blog-title".to_string())
    }
}