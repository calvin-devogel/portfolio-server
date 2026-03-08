use actix_web::{HttpResponse, web};
use anyhow::Context;
use sqlx::PgPool;
use totp_rs::{Algorithm, Secret, TOTP};

use crate::authentication::UserId;
use crate::utils::{e500, encrypt_totp_secret};

#[tracing::instrument(name = "TOTP setup", skip(pool, user_id))]
pub async fn totp_setup(
    pool: web::Data<PgPool>,
    user_id: web::ReqData<UserId>,
) -> Result<HttpResponse, actix_web::Error> {
    let user_id = user_id.into_inner();

    // generate a secret and encode
    let secret = Secret::generate_secret();
    let secret_b32 = secret.to_encoded().to_string();

    let status = sqlx::query!(
        "SELECT totp_enabled FROM users WHERE user_id = $1",
        *user_id
    )
    .fetch_one(pool.as_ref())
    .await
    .context("Failed to get totp status")
    .map_err(e500)?;

    if status.totp_enabled {
        return Ok(HttpResponse::Conflict().finish())
    }

    sqlx::query!(
        "UPDATE users SET totp_secret = $1 WHERE user_id = $2",
        secret_b32,
        *user_id,
    )
    .execute(pool.as_ref())
    .await
    .context("Failed to store pending TOTP secret")
    .map_err(e500)?;

    let totp = TOTP::new(
        Algorithm::SHA1,
        6,
        1,
        30,
        secret.to_bytes().map_err(e500)?,
        None,
        user_id.to_string(),
    )
    .map_err(e500)?;

    let otpauth_uri = totp.get_url();

    Ok(HttpResponse::Ok().json(serde_json::json!({ "otpauth_uri": otpauth_uri })))
}
