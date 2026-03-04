use actix_web::{HttpRequest, HttpResponse, web};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::{
    authentication::UserId,
    errors::BlogError, idempotency::execute_idempotent,
    types::article::{ArticleForm, ArticleId, ArticleResponse}
};

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
    let sections_json = article
        .sections_as_json()
        .map_err(|e|
            BlogError::UnexpectedError(anyhow::anyhow!("Failed to serialize sections: {e:?}")
        ))?;
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

fn get_article_slug(title: &str) -> String {
    title.replace(' ', "-").to_ascii_lowercase()
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
