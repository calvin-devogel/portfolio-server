// read mfa_pending_user_id from session
// load totp_secret
// use totp-rs to verify (+/- 1 window for clock slew)
// if valid: session.clear_mfa_pending(); session.insert_user_id(user_id); return 200 (plus?)
// if invalid: 401, do not clear pending session

use actix_web::{HttpResponse, web};
use anyhow::Context;
use sqlx::PgPool;
use totp_rs::{Algorithm, Secret, TOTP};

use crate::session_state::TypedSession;
use crate::startup::TotpEncryptionKey;
use crate::utils::e500;
use crate::types::user::UserRole;

#[derive(serde::Deserialize, Debug)]
pub struct VerifyTotpRequest {
    code: String,
}

#[allow(clippy::future_not_send)]
#[tracing::instrument(
    name = "Verify TOTP code",
    skip(pool, session, request, encryption_key)
)]
pub async fn verify_totp(
    request: web::Json<VerifyTotpRequest>,
    pool: web::Data<PgPool>,
    session: TypedSession,
    encryption_key: web::Data<TotpEncryptionKey>,
) -> Result<HttpResponse, actix_web::Error> {
    let user_id = session
        .get_mfa_pending_user_id()
        .map_err(e500)?
        .ok_or_else(|| actix_web::error::ErrorUnauthorized("No MFA session in progress"))?;

    let (encrypted, user_role) = get_totp_secret_and_role(user_id, &pool)
        .await
        .map_err(e500)?
        .ok_or_else(|| {
            actix_web::error::ErrorUnauthorized("TOTP not configured for user: {user_id}")
        })?;

    let totp_secret =
        String::from_utf8(crate::crypto::decrypt(&encryption_key.0, &encrypted).map_err(e500)?)
            .map_err(e500)?;

    let totp = TOTP::new(
        Algorithm::SHA1,
        6,
        1,
        30,
        Secret::Encoded(totp_secret).to_bytes().map_err(e500)?,
        None,
        user_id.to_string(),
    )
    .map_err(e500)?;

    if totp.check_current(&request.code).map_err(e500)? {
        session.clear_mfa_pending();
        session.insert_user_id(user_id).map_err(e500)?;
        session.insert_user_role(user_role).map_err(e500)?;
        Ok(HttpResponse::Ok().finish())
    } else {
        Ok(HttpResponse::Unauthorized().finish())
    }
}

async fn get_totp_secret_and_role(
    user_id: uuid::Uuid,
    pool: &PgPool,
) -> Result<Option<(Vec<u8>, UserRole)>, anyhow::Error> {
    let row = sqlx::query!(
        r#"SELECT totp_secret, role::TEXT FROM users WHERE user_id = $1"#,
        user_id
    )
    .fetch_one(pool)
    .await
    .context("Failed to fetch TOTP secret")?;

    let user_role = UserRole::from_str(&row.role.unwrap_or_else(|| "User".to_string()))
        .unwrap_or(UserRole::User);

    Ok(row.totp_secret.map(|secret| (secret, user_role)))
}
