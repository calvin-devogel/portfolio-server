use actix_web::{HttpResponse, web};
use anyhow::Context;
use sqlx::PgPool;

use crate::authentication::UserId;
use crate::utils::e500;

#[tracing::instrument(name = "TOTP status", skip(pool, user_id))]
pub async fn totp_status(
    pool: web::Data<PgPool>,
    user_id: web::ReqData<UserId>,
) -> Result<HttpResponse, actix_web::Error> {
    let user_id = user_id.into_inner();

    let status = sqlx::query!(
        "SELECT totp_enabled FROM users WHERE user_id = $1",
        *user_id
    )
    .fetch_one(pool.as_ref())
    .await
    .context("Failed to retrieve totp status")
    .map_err(e500)?;

    Ok(HttpResponse::Ok().json(serde_json::json!({ "totp_enabled": status.totp_enabled })))
}
