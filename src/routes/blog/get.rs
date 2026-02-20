use actix_web::{HttpResponse, web};
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::{
    errors::BlogError,
    pagination::{PaginatedResponse, PaginationMeta, PaginationQuery}
};

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

#[tracing::instrument(
    name = "Get blog posts with pagination",
    skip(pool),
    fields(page = %query.page, page_size = %query.page_size)
)]
pub async fn get_blog_posts(
    query: web::Query<PaginationQuery>,
    pool: web::Data<PgPool>,
) -> Result<HttpResponse, actix_web::Error> {
    let q = query.into_inner();
    let offset = q.offset();
    let page_size = q.page_size();

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

    let response = PaginatedResponse {
        data: blog_posts,
        pagination: PaginationMeta::from_total(total_count, &q)
    };

    Ok(HttpResponse::Ok().json(response))
}