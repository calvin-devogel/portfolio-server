use actix_web::{HttpResponse};
// use sqlx::PgPool;

// #[derive(thiserror::Error, Debug)]
// pub enum GetMessageError {
//     #[error(transparent)]
//     UnexpectedError(#[from] anyhow::Error),
// }

// #[derive(serde::Deserialize)]
// pub struct PaginationParams {
//     page: i64,
//     per_page: i64,
// }

pub async fn get_messages() -> HttpResponse {
    HttpResponse::Ok().finish()
}

// #[tracing::instrument(
//     name = "Get messages from contact table",
//     skip(pool, params)
// )]
// // we want to paginate these results so we're selecting only a certain amount per page
// // this means we need to know:
// // 1) which page we're on
// // 2) how many messages per page there are at most
// pub async fn get_messages(
//     pool: web::Data<PgPool>,
//     params: web::Query<PaginationParams>
// ) -> Result<HttpResponse, actix_web::Error> {
//     let offset = (params.page - 1) * params.per_page;
//     let total = sqlx::query_scalar!(
//         r#"SELECT COUNT(*) FROM messages"#
//     )
//     .fetch_one(pool.as_ref()).await;

//     let page = sqlx::query!(
//         r#"
//         SELECT message_id, email, message_text, created_at, read_message
//         FROM messages
//         ORDER BY created_at DESC
//         LIMIT $1
//         OFFSET $2
//         "#,
//         params.per_page,
//         offset
//     )
//     .fetch_all(pool.as_ref())
//     .await
//     .map_err(|e| {
//         tracing::error!("Failed to fetch messages {e:?}");
//         GetMessageError::UnexpectedError(anyhow::anyhow!("Failed to fetch messages"));
//     })?;
// }