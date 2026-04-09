use sqlx::{PgExecutor, Postgres, QueryBuilder};
use uuid::Uuid;

use super::models::{ArticleRecord, ArticleRecordRaw};

pub async fn insert_article_query<'a>(
    executor: impl PgExecutor<'a>,
    post_id: Uuid,
    title: &str,
    slug: &str,
    sections_json: serde_json::Value,
    excerpt: &str,
    author: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        r#"
        INSERT INTO blog_posts(
        post_id, title, slug, sections, excerpt, author, published, created_at, updated_at)
        VALUES ($1, $2, $3, $4, $5, $6, FALSE, NOW(), NOW())"#,
        post_id,
        title,
        slug,
        sections_json,
        excerpt,
        author
    )
    .execute(executor)
    .await?;

    Ok(())
}

pub async fn delete_article_query<'a>(
    executor: impl PgExecutor<'a>,
    post_id: Uuid,
) -> Result<u64, sqlx::Error> {
    let result = sqlx::query!("DELETE FROM blog_posts WHERE post_id = $1", post_id)
        .execute(executor)
        .await?;

    Ok(result.rows_affected())
}

pub async fn publish_article_query<'a>(
    executor: impl PgExecutor<'a>,
    post_id: Uuid,
    is_published: bool,
) -> Result<u64, sqlx::Error> {
    let result = sqlx::query!(
        r#"
        UPDATE blog_posts
        SET published = $2, updated_at = NOW()
        WHERE post_id = $1"#,
        post_id,
        is_published
    )
    .execute(executor)
    .await?;

    Ok(result.rows_affected())
}

pub async fn update_article_query<'a>(
    executor: impl PgExecutor<'a>,
    post_id: Uuid,
    title: Option<String>,
    excerpt: Option<String>,
    author: Option<String>,
    sections_json: Option<serde_json::Value>,
) -> Result<u64, sqlx::Error> {
    let mut builder = QueryBuilder::<Postgres>::new("UPDATE blog_posts SET ");
    let mut separator = builder.separated(", ");
    let mut has_updates = false;

    // macro to clean up builder pattern
    macro_rules! push_if_some {
        ($field:expr, $col:literal) => {
            if let Some(val) = $field {
                separator.push(concat!($col, " = "));
                separator.push_bind_unseparated(val);
                has_updates = true;
            }
        };
    }

    push_if_some!(title, "title");
    push_if_some!(excerpt, "excerpt");
    push_if_some!(author, "author");

    if let Some(sections) = sections_json {
        separator.push("sections = ");
        separator.push_bind_unseparated(sections);
        has_updates = true;
    }

    // only append updated_at if a field actually has updates
    if !has_updates {
        return Ok(0);
    }

    builder.push(", updated_at = NOW() WHERE post_id = ");
    builder.push_bind(post_id);

    let result = builder.build().execute(executor).await?;
    Ok(result.rows_affected())
}

pub async fn count_articles<'a>(
    executor: impl PgExecutor<'a>,
    on_published: bool,
    slug: Option<&str>,
) -> Result<i64, sqlx::Error> {
    let total_count = sqlx::query!(
        r#"
        SELECT COUNT(*) FROM blog_posts
        WHERE (NOT $1 OR published = true)
        AND ($2::text IS NULL OR slug = $2)
        "#,
        on_published,
        slug
    )
    .fetch_one(executor)
    .await?;

    Ok(total_count.count.unwrap_or(0))
}

pub async fn fetch_articles<'a>(
    executor: impl PgExecutor<'a>,
    on_published: bool,
    slug: Option<&str>,
    limit: i64,
    offset: i64,
) -> Result<Vec<ArticleRecord>, sqlx::Error> {
    let articles: Vec<ArticleRecord> = sqlx::query_as!(
        ArticleRecordRaw,
        r#"
        SELECT
            post_id, title, slug, sections as "sections: serde_json::Value",
            excerpt, author, published, created_at, updated_at
        FROM blog_posts
        WHERE (NOT $1 OR published = true)
        AND ($2::text IS NULL OR slug = $2)
        ORDER BY created_at DESC
        LIMIT $3 OFFSET $4"#,
        on_published,
        slug,
        limit,
        offset
    )
    .fetch_all(executor)
    .await?
    .into_iter()
    .map(ArticleRecord::try_from)
    .collect::<Result<Vec<_>, _>>()
    .map_err(|e| sqlx::Error::Decode(Box::new(e)))?;

    Ok(articles)
}
