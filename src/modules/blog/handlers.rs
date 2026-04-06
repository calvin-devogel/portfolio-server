use actix_web::{HttpRequest, HttpResponse, web};
use sqlx::{PgPool, Postgres, QueryBuilder, Transaction};
use uuid::Uuid;

use crate::{
    errors::BlogError, api::idempotency::execute_idempotent, modules::auth::{UserId, TypedSession}, core::{PaginationQuery, PaginationMeta, PaginatedResponse},
};

use super::models::{ArticleEditRequest, ArticlePublishRequest, ArticleForm, ArticleId, ArticleResponse, ArticleDeleteRequest, ArticleRecord, ArticleRecordRaw};

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
        return Err(BlogError::BadRequest(anyhow::anyhow!("No fields provided to update")).into());
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

#[tracing::instrument(
    name = "Insert blog post",
    skip(blog_post, pool, request, user_id),
    fields(
        post_id = tracing::field::Empty
    )
)]
pub async fn insert_article(
    blog_post: web::Json<ArticleForm>,
    user_id: web::ReqData<UserId>,
    pool: web::Data<PgPool>,
    request: HttpRequest,
) -> Result<HttpResponse, actix_web::Error> {
    let blog_to_post = blog_post.into_inner();
    let user_id = Some(**user_id);

    blog_to_post.validate().map_err(actix_web::Error::from)?;

    execute_idempotent(&request, &pool, user_id, move |tx| {
        Box::pin(async move { process_new_article(tx, blog_to_post).await })
    })
    .await
}

#[allow(clippy::future_not_send)]
async fn process_new_article(
    transaction: &mut Transaction<'static, Postgres>,
    article: ArticleForm,
) -> Result<HttpResponse, actix_web::Error> {
    let post_id = ArticleId(Uuid::new_v4());
    let slug = get_article_slug(&article.title);
    let sections_json = article.sections_as_json().map_err(|e| {
        BlogError::UnexpectedError(anyhow::anyhow!("Failed to serialize sections: {e:?}"))
    })?;
    tracing::Span::current().record("post_id", tracing::field::display(&post_id));

    let insert_result = sqlx::query!(
        r#"
        INSERT INTO blog_posts(
        post_id,
        title,
        slug,
        sections,
        excerpt,
        author,
        published,
        created_at,
        updated_at)
        VALUES ($1, $2, $3, $4, $5, $6, FALSE, NOW(), NOW())"#,
        *post_id,
        article.title,
        slug,
        sections_json,
        article.excerpt,
        article.author
    )
    .execute(transaction.as_mut())
    .await;

    match insert_result {
        Ok(_) => {
            tracing::info!("Post saved successfully with: {}", post_id);
            Ok(HttpResponse::Accepted()
                .json(ArticleResponse::new("Post received successfully", post_id)))
        }
        Err(e) => {
            if let sqlx::Error::Database(db_err) = &e
                && db_err.code().as_deref() == Some("23505")
            {
                tracing::warn!("Duplicate post detected");
                return Err(BlogError::DuplicatePost.into());
            }

            tracing::error!("Failed to save post: {e:?}");
            Err(BlogError::UnexpectedError(anyhow::anyhow!("Posting blog failed: {e:?}")).into())
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
    skip(pool, session),
    fields(page, page_size, on_published, slug)
)]
pub async fn get_articles(
    request: HttpRequest,
    pool: web::Data<PgPool>,
    session: TypedSession,
) -> Result<HttpResponse, actix_web::Error> {
    let pagination = PaginationQuery {
        page: parse_header(&request, "BlogPost-Page").unwrap_or(1),
        page_size: parse_header(&request, "BlogPost-Page-Size").unwrap_or(20),
    };

    let is_authenticated = session
        .get_user_id()
        .map_err(|e| BlogError::UnexpectedError(anyhow::anyhow!(e)))?
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

    let articles: Vec<ArticleRecord> = sqlx::query_as!(
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
        data: articles,
        pagination: PaginationMeta::from_total(total_count, &pagination),
    };

    Ok(HttpResponse::Ok().json(response))
}