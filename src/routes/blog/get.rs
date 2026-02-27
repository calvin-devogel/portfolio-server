use actix_web::{HttpRequest, HttpResponse, web};
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::{
    errors::BlogError,
    pagination::{PaginatedResponse, PaginationMeta, PaginationQuery},
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

fn parse_header_str<'a>(req: &'a HttpRequest, key: &str) -> Option<&'a str> {
    req.headers().get(key)?.to_str().ok()
}

fn parse_header<T: std::str::FromStr>(req: &HttpRequest, key: &str) -> Option<T> {
    parse_header_str(req, key)?.parse().ok()
}

#[tracing::instrument(
    name = "Get blog posts with pagination",
    skip(pool),
    fields(
        page,
        page_size,
        on_published,
        slug,
    )
)]
pub async fn get_blog_posts(
    request: HttpRequest,
    pool: web::Data<PgPool>,
) -> Result<HttpResponse, actix_web::Error> {
    let pagination = PaginationQuery {
        page: parse_header(&request, "BlogPost-Page").unwrap_or(1),
        page_size: parse_header(&request, "BlogPost-Page-Size").unwrap_or(20),
    };

    let on_published = parse_header(&request, "BlogPost-On-Published").unwrap_or(false);
    let slug: Option<String> = parse_header_str(&request, "BlogPost-Slug").map(str::to_owned);

    tracing::Span::current()
        .record("page", pagination.page)
        .record("page size", pagination.page_size)
        .record("on_published", on_published)
        .record("slug", slug.as_deref().unwrap_or("no slug"));

    let total_count = sqlx::query_scalar!(
        r#"
        SELECT COUNT(*)
        FROM blog_posts 
        WHERE 
            (NOT $1 OR published = true)
            AND ($2::text IS NULL OR slug = $2)
        "#,
        on_published,
        slug
    )
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
        SELECT
            post_id,
            title,
            slug,
            content,
            excerpt,
            author,
            published,
            created_at,
            updated_at
        FROM blog_posts
        WHERE
            (NOT $1 OR published = true)
            AND ($2::text IS NULL OR slug = $2)
        ORDER BY created_at DESC
        LIMIT $3 OFFSET $4"#,
        on_published,
        slug,
        pagination.page_size,
        pagination.offset()
    )
    .fetch_all(pool.as_ref())
    .await
    .map_err(|e| {
        tracing::error!("Failed to fetch blog posts: {e:?}");
        BlogError::UnexpectedError(anyhow::anyhow!(e))
    })?;

    let response = PaginatedResponse {
        data: blog_posts,
        pagination: PaginationMeta::from_total(total_count, &pagination),
    };

    Ok(HttpResponse::Ok().json(response))
}
