use actix_web::{HttpRequest, HttpResponse, web};
use sqlx::PgPool;

use crate::{
    errors::BlogError,
    pagination::{PaginatedResponse, PaginationMeta, PaginationQuery},
    types::blog::{ArticleRecord, ArticleRecordRaw}
};

// TODO: content should change to an array of "type" entries called "sections",
// communicating to the client what type of section each entry is
// (markdown/carousel/maybe others?)

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
pub async fn get_articles(
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

    let blog_posts: Vec<ArticleRecord> = sqlx::query_as!(
        ArticleRecordRaw,
        r#"
        SELECT
            post_id,
            title,
            slug,
            sections as "sections: serde_json::Value",
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
    })?
    .into_iter()
    .map(ArticleRecord::try_from)
    .collect::<Result<Vec<_>, _>>()
    .map_err(|e| {
        tracing::error!("Failed to deserialize blog post sections: {e:?}");
        BlogError::UnexpectedError(anyhow::anyhow!(e))
    })?;

    let response = PaginatedResponse {
        data: blog_posts,
        pagination: PaginationMeta::from_total(total_count, &pagination),
    };

    Ok(HttpResponse::Ok().json(response))
}
