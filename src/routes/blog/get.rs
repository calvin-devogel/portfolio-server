use actix_web::{HttpResponse, web};
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::BlogError;

#[derive(serde::Deserialize, Debug)]
pub struct BlogPostQuery {
    #[serde(default)]
    page: i64,
    #[serde(default = "default_page_size")]
    page_size: i64
}

const fn default_page_size() -> i64 {
    5
}

#[derive(serde::Serialize)]
struct BlogPostRecord {
    post_id: Uuid,
    title: String,
    slug: String,
    content: String,
    excerpt: String,
    author: String,
    published: bool,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[derive(serde::Serialize)]
struct BlogPostsResponse {
    blog_posts: Vec<BlogPostRecord>,
    page: i64,
    page_size: i64, 
    total_count: i64,
}

#[tracing::instrument(
    name = "Get blog posts with pagination",
    skip(pool),
    fields(page = %query.page, page_size = %query.page_size)
)]
pub async fn get_blog_posts(
    query: web::Query<BlogPostQuery>,
    pool: web::Data<PgPool>,
) -> Result<HttpResponse, actix_web::Error> {
    let page = query.page.max(0);
    let page_size = query.page_size.clamp(1, 5);
    let offset = page * page_size;

    let total_count = sqlx::query_scalar!("SELECT COUNT(*) FROM blog_posts")
        .fetch_one(pool.as_ref())
        .await
        .map_err(|e| {
            tracing::error!("Failed to get blog post count: {e:?}");
            BlogError::QueryFailed
        })?
        .unwrap_or(0);

    let blog_posts = sqlx::query_as!(
        BlogPostRecord,
        r#"
        SELECT post_id, title, slug, content, excerpt, author, published, created_at, updated_at
        FROM blog_posts
        ORDER BY created_at DESC
        LIMIT $1 OFFSET $2"#,
        page_size,
        offset
    )
    .fetch_all(pool.as_ref())
    .await
    .map_err(|e| {
        tracing::error!("Failed to fetch blog posts: {e:?}");
        BlogError::UnexpectedError(anyhow::anyhow!(e))
    })?;

    tracing::info!(
        "Retrieved {} blog posts for page {} (page_size: {})",
        blog_posts.len(),
        page,
        page_size
    );

    Ok(HttpResponse::Ok().json(BlogPostsResponse {
        blog_posts,
        page,
        page_size,
        total_count
    }))
}