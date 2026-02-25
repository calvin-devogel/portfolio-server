use actix_web::{HttpRequest, HttpResponse, web};
use sqlx::{PgPool, Postgres, Transaction};
use std::ops::Deref;
use uuid::Uuid;

use crate::{authentication::UserId, errors::BlogError, idempotency::execute_idempotent};

#[derive(serde::Deserialize)]
pub struct BlogPostForm {
    title: String,
    content: String,
    excerpt: String,
    author: String,
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
        Self { message, post_id }
    }
}

#[tracing::instrument(
    name = "Insert blog post",
    skip(blog_post, pool, request, user_id),
    fields(
        post_id = tracing::field::Empty
    )
)]
pub async fn insert_blog_post(
    blog_post: web::Form<BlogPostForm>,
    user_id: web::ReqData<UserId>,
    pool: web::Data<PgPool>,
    request: HttpRequest,
) -> Result<HttpResponse, actix_web::Error> {
    let blog_to_post = blog_post.0;
    let user_id = Some(**user_id);

    execute_idempotent(&request, &pool, user_id, move |tx| {
        Box::pin(async move { process_new_blog_post(tx, blog_to_post).await })
    })
    .await
}

#[allow(clippy::future_not_send)]
async fn process_new_blog_post(
    transaction: &mut Transaction<'static, Postgres>,
    blog_post: BlogPostForm,
) -> Result<HttpResponse, actix_web::Error> {
    let post_id = BlogPostId(Uuid::new_v4());
    let slug = get_blog_post_slug(&blog_post.title);
    tracing::Span::current().record("post_id", tracing::field::display(&post_id));

    let insert_result = sqlx::query!(
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
    .execute(transaction.as_mut())
    .await;

    match insert_result {
        Ok(_) => {
            tracing::info!("Post saved successfully with: {}", post_id);
            Ok(HttpResponse::Accepted()
                .json(BlogPostResponse::new("Post received successfully", post_id)))
        }
        Err(e) => {
            if let sqlx::Error::Database(db_err) = &e {
                if db_err.code().as_deref() == Some("23505") {
                    tracing::warn!("Duplicate post detected");
                    return Err(BlogError::DuplicatePost.into());
                }
            }

            tracing::error!("Failed to save post: {e:?}");
            Err(BlogError::UnexpectedError(anyhow::anyhow!("Posting blog failed: {e:?}")).into())
        }
    }
}

fn get_blog_post_slug(title: &str) -> String {
    title.replace(" ", "-").to_ascii_lowercase()
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
