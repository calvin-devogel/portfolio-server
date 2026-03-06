use actix_web::{HttpResponse, web};
use anyhow::Context;
use secrecy::SecretString;
use sqlx::PgPool;

use crate::authentication::{Credentials, UserId, validate_credentials};
use crate::utils::e500;

#[derive(serde::Deserialize, Debug)]
pub struct DisableTotpRequest {
    password: SecretString,
}

#[tracing::instrument(name = "TOTP disable", skip(pool, user_id, request))]
pub async fn totp_disable(
    request: web::Json<DisableTotpRequest>,
    pool: web::Data<PgPool>,
    user_id: web::ReqData<UserId>,
) -> Result<HttpResponse, actix_web::Error> {
    let user_id = user_id.into_inner();

    let username = sqlx::query_scalar!("SELECT username FROM users WHERE user_id = $1", *user_id,)
        .fetch_one(pool.as_ref())
        .await
        .context("Failed to fetch username")
        .map_err(e500)?;

    // revalidate before allowing removal
    let credentials = Credentials {
        username,
        password: request.into_inner().password,
    };

    validate_credentials(credentials, &pool)
        .await
        .map_err(|_| actix_web::error::ErrorUnauthorized("Invalid password"))?;

    sqlx::query!(
        "UPDATE users SET totp_secret = NULL, totp_enabled = FALSE WHERE user_id = $1",
        *user_id,
    )
    .execute(pool.as_ref())
    .await
    .context("Failed to disable TOTP")
    .map_err(e500)?;

    Ok(HttpResponse::Ok().finish())
}
