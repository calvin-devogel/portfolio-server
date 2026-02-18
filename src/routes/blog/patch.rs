// start easy, just update published flag
use actix_web::{HttpRequest, HttpResponse, web};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::{
    authentication::UserId,
    // blog error module: make one
    idempotency::{IdempotencyKey, NextAction, get_idempotency_key, save_response, try_processing},
};

#[derive(serde::Deserialize)]
pub struct BlogPatchRequest {
    blog_post_id: Uuid,
    published: bool,
}

#[tracing::instrument(
    name = "Publish blog post",
    skip_all,
    fields(user_id = %*user_id, blog_post_id = %blog_post.blog_post_id)
)]
pub async fn publish_blog_post(
    blog_patch: web::Json<BlogPatchRequest>,
    user_id: web::ReqData<UserId>,
    reqeust: HttpRequest,
    pool: web::Data<PgPool>
) -> Result<HttpResponse, actix_web::Error> {
    let idempotency_key: IdempotencyKey = get_idempotency_key(request)
        .map_err(|e| {
            tracing::warn!(error = ?e, "Failed to get idempotency key")
        })?;
        
}