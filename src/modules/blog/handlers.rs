use actix_web::{HttpRequest, HttpResponse, http::StatusCode, web};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::{
    api::idempotency::execute_idempotent,
    core::{PaginatedResponse, PaginationMeta, PaginationQuery, error::ArticleError},
    modules::auth::{TypedSession, UserId, UserRole},
    modules::metrics::AppMetrics,
};

use super::models::{
    ArticleDeleteRequest, ArticleEditRequest, ArticleForm, ArticleId, ArticlePublishRequest,
    ArticleResponse,
};

use super::db::{
    count_articles, delete_article_query, fetch_articles, insert_article_query,
    publish_article_query, update_article_query,
};

#[tracing::instrument(
    name = "Delete blog post",
    skip_all,
    fields(user_id = %*user_id, article_id = %article.post_id)
)]
#[allow(clippy::future_not_send)]
pub async fn delete_article(
    article: web::Json<ArticleDeleteRequest>,
    user_id: UserId,
    request: HttpRequest,
    pool: web::Data<PgPool>,
) -> Result<HttpResponse, ArticleError> {
    let article_to_delete = article.0;

    execute_idempotent(&request, &pool, Some(*user_id), move |tx| {
        Box::pin(async move { process_delete_article(tx, article_to_delete).await })
    })
    .await
}

async fn process_delete_article(
    transaction: &mut Transaction<'static, Postgres>,
    article: ArticleDeleteRequest,
) -> Result<HttpResponse, ArticleError> {
    let rows = delete_article_query(transaction.as_mut(), article.post_id)
        .await
        .map_err(|e| ArticleError::Unexpected(e.into()))?;

    handle_rows_affected(rows, article.post_id, StatusCode::OK, "deleted")
}

#[tracing::instrument(name = "Edit blog post", skip_all)]
#[allow(clippy::future_not_send)]
pub async fn edit_article(
    article_edit_request: web::Json<ArticleEditRequest>,
    user_id: UserId,
    request: HttpRequest,
    pool: web::Data<PgPool>,
) -> Result<HttpResponse, ArticleError> {
    let article_to_edit = article_edit_request.into_inner();

    article_to_edit.validate()?;

    execute_idempotent(&request, &pool, Some(*user_id), move |tx| {
        Box::pin(async move { process_edit_article(tx, article_to_edit).await })
    })
    .await
}

async fn process_edit_article(
    transaction: &mut Transaction<'static, Postgres>,
    article: ArticleEditRequest,
) -> Result<HttpResponse, ArticleError> {
    let sections_json = article
        .sections_as_json()
        .map_err(|e| ArticleError::Unexpected(e.into()))?;

    if article.title.is_none()
        && article.excerpt.is_none()
        && article.author.is_none()
        && sections_json.is_none()
    {
        return Err(ArticleError::BadRequest(
            "No fields provided to update".into(),
        ));
    }

    let rows = update_article_query(
        transaction.as_mut(),
        article.post_id,
        article.title,
        article.excerpt,
        article.author,
        sections_json,
    )
    .await
    .map_err(|e| ArticleError::Unexpected(e.into()))?;

    handle_rows_affected(rows, article.post_id, StatusCode::ACCEPTED, "updated")
}

#[tracing::instrument(name = "Publish blog post", skip_all)]
#[allow(clippy::future_not_send)]
pub async fn publish_article(
    article: web::Json<ArticlePublishRequest>,
    user_id: UserId,
    request: HttpRequest,
    pool: web::Data<PgPool>,
) -> Result<HttpResponse, ArticleError> {
    let article_to_publish = article.0;

    execute_idempotent(&request, &pool, Some(*user_id), move |tx| {
        Box::pin(async move { process_publish_article(tx, article_to_publish).await })
    })
    .await
}

async fn process_publish_article(
    transaction: &mut Transaction<'static, Postgres>,
    article: ArticlePublishRequest,
) -> Result<HttpResponse, ArticleError> {
    let rows = publish_article_query(transaction.as_mut(), article.post_id, article.published)
        .await
        .map_err(|e| ArticleError::Unexpected(e.into()))?;

    handle_rows_affected(rows, article.post_id, StatusCode::ACCEPTED, "published")
}

#[tracing::instrument(
    name = "Insert blog post",
    skip(blog_post, pool, request, user_id),
    fields(
        post_id = tracing::field::Empty
    )
)]
#[allow(clippy::future_not_send)]
pub async fn insert_article(
    blog_post: web::Json<ArticleForm>,
    user_id: UserId,
    pool: web::Data<PgPool>,
    request: HttpRequest,
) -> Result<HttpResponse, ArticleError> {
    let blog_to_post = blog_post.into_inner();

    blog_to_post.validate()?;

    execute_idempotent(&request, &pool, Some(*user_id), move |tx| {
        Box::pin(async move { process_new_article(tx, blog_to_post).await })
    })
    .await
}

async fn process_new_article(
    transaction: &mut Transaction<'static, Postgres>,
    article: ArticleForm,
) -> Result<HttpResponse, ArticleError> {
    let post_id = Uuid::new_v4();
    let slug = get_article_slug(&article.title);
    let sections_json = article
        .sections_as_json()
        .map_err(|e| ArticleError::Unexpected(e.into()))?;

    tracing::Span::current().record("post_id", tracing::field::display(&post_id));

    let insert_result = insert_article_query(
        transaction.as_mut(),
        post_id,
        &article.title,
        &slug,
        sections_json,
        &article.excerpt,
        &article.author,
    )
    .await;

    match insert_result {
        Ok(()) => {
            tracing::info!("Post saved successfully with: {}", post_id);
            Ok(HttpResponse::Accepted().json(ArticleResponse::new(
                "Post received successfully",
                ArticleId(post_id),
            )))
        }
        Err(sqlx::Error::Database(db_err)) if db_err.code().as_deref() == Some("23505") => {
            tracing::warn!("Duplicate post detected");
            Err(ArticleError::DuplicatePost)
        }
        Err(e) => {
            tracing::error!("Failed to save post: {e:?}");
            Err(ArticleError::Unexpected(e.into()))
        }
    }
}

fn get_article_slug(title: &str) -> String {
    title
        .replace(' ', "-")
        .chars()
        .filter(|c| c.is_ascii_alphabetic() || *c == '-')
        .collect::<String>()
        .to_ascii_lowercase()
}

fn handle_rows_affected(
    rows: u64,
    post_id: Uuid,
    success_status: StatusCode,
    context: &str,
) -> Result<HttpResponse, ArticleError> {
    match rows {
        1 => {
            tracing::info!("Post {} {} successfully", post_id, context);
            Ok(HttpResponse::build(success_status).finish())
        }
        0 => {
            tracing::warn!("Blog post not found: {}", post_id);
            Err(ArticleError::NotFound)
        }
        rows => {
            tracing::error!(
                "Unexpected rows affected: {} for post id: {}",
                rows,
                post_id
            );
            Err(ArticleError::Unexpected(anyhow::anyhow!(
                "Unexpected rows affected: {rows}"
            )))
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn article_slug() {
        let title = "New Blog Title".to_string();
        let slug = get_article_slug(&title);
        assert_eq!(slug, "new-blog-title".to_string())
    }
}

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
    skip(pool, session, app_metrics),
    fields(page, page_size, on_published, slug)
)]
#[allow(clippy::future_not_send)]
pub async fn get_articles(
    request: HttpRequest,
    pool: web::Data<PgPool>,
    session: TypedSession,
    app_metrics: web::Data<AppMetrics>,
) -> Result<HttpResponse, actix_web::Error> {
    let pagination = PaginationQuery {
        page: parse_header(&request, "BlogPost-Page").unwrap_or(1),
        page_size: parse_header(&request, "BlogPost-Page-Size").unwrap_or(20),
    };

    let is_authenticated = session
        .get_user_id()
        .map_err(|e| ArticleError::Unexpected(e.into()))?
        .is_some();

    let on_published = if is_authenticated {
        parse_header(&request, "BlogPost-OnPublished").unwrap_or(false)
    } else {
        true
    };

    let slug: Option<String> = parse_header_str(&request, "BlogPost-Slug").map(str::to_owned);

    tracing::Span::current()
        .record("page", pagination.page)
        .record("page size", pagination.page_size)
        .record("on_published", on_published)
        .record("slug", slug.as_deref().unwrap_or("no slug"));

    let total_count = count_articles(pool.as_ref(), on_published, slug.as_deref())
        .await
        .map_err(|e| {
            tracing::error!("Failed to get blog post count: {e:?}");
            ArticleError::Unexpected(e.into())
        })?;

    let articles = fetch_articles(
        pool.as_ref(),
        on_published,
        slug.as_deref(),
        pagination.page_size,
        pagination.offset(),
    )
    .await
    .map_err(|e| {
        tracing::error!("Failed to fetch or deserialize blog posts: {e:?}");
        ArticleError::Unexpected(e.into())
    })?;

    let response = PaginatedResponse {
        data: articles,
        pagination: PaginationMeta::from_total(total_count, &pagination),
    };

    let is_admin = session
        .get_user_role()
        .ok()
        .flatten()
        .map(|r| r == UserRole::Admin)
        .unwrap_or(false);

    if !is_admin {
        app_metrics.blog_views_total.inc();
    }

    Ok(HttpResponse::Ok().json(response))
}
