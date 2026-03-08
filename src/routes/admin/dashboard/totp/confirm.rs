use actix_web::{HttpResponse, web};
use anyhow::Context;
use sqlx::PgPool;
use totp_rs::{Algorithm, Secret, TOTP};

use crate::authentication::UserId;
use crate::utils::e500;

#[derive(serde::Deserialize, Debug)]
pub struct ConfirmTotpRequest {
    code: String,
}

#[tracing::instrument(name = "TOTP confirm", skip(pool, user_id, request))]
pub async fn totp_confirm(
    request: web::Json<ConfirmTotpRequest>,
    pool: web::Data<PgPool>,
    user_id: web::ReqData<UserId>,
) -> Result<HttpResponse, actix_web::Error> {
    let user_id = user_id.into_inner();

    let row = sqlx::query!(
        "SELECT totp_secret, totp_enabled FROM users WHERE user_id =  $1",
        *user_id,
    )
    .fetch_one(pool.as_ref())
    .await
    .context("Failed to fetch TOTP state")
    .map_err(e500)?;

    // reject if already enabled or no secret
    if row.totp_enabled {
        return Ok(HttpResponse::Conflict().finish());
    }
    let secret_b32 = row
        .totp_secret
        .ok_or_else(|| actix_web::error::ErrorBadRequest("No TOTP setup in progress"))?;

    let totp = TOTP::new(
        Algorithm::SHA1,
        6,
        1,
        30,
        Secret::Encoded(secret_b32).to_bytes().map_err(e500)?,
        None,
        user_id.to_string(),
    )
    .map_err(e500)?;

    if !totp.check_current(&request.code).map_err(e500)? {
        return Ok(HttpResponse::Unauthorized().finish());
    }

    sqlx::query!(
        "UPDATE users SET totp_enabled = TRUE WHERE user_id = $1",
        *user_id
    )
    .execute(pool.as_ref())
    .await
    .context("Failed to enable TOTP")
    .map_err(e500)?;

    Ok(HttpResponse::Ok().finish())
}
