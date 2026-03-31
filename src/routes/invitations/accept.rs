use actix_web::{web, HttpResponse};
use sqlx::PgPool;
use sha2::{Sha256, Digest};
use secrecy::{ExposeSecret, SecretString};
use crate::{authentication::compute_password_hash};

#[derive(serde::Deserialize)]
pub struct AcceptInvitationParams {
    token: String,
    username: String,
    password: String, // add a validator to both front and backend
}

#[tracing::instrument(name = "Accept user invitation", skip_all)]
pub async fn accept_invitation(
    params: web::Json<AcceptInvitationParams>,
    pool: web::Data<PgPool>,
) -> Result<HttpResponse, actix_web::Error> {
    let mut hasher = Sha256::new();
    hasher.update(params.token.as_bytes());
    let token_hash = hex::encode(hasher.finalize());

    // don't need idempotency here since invitation accepts are one-time
    let mut tx = pool.begin().await
        .map_err(actix_web::error::ErrorInternalServerError)?;

    let invitation = sqlx::query!(
        r#"
        SELECT id, email, role FROM user_invitations
        WHERE invitation_token_hash = $1
            AND consumed_at IS NULL
            AND expires_at > NOW()
        "#,
        token_hash,
    )
    .fetch_optional(&mut *tx)
    .await
    .map_err(actix_web::error::ErrorInternalServerError)?
    .ok_or_else(|| actix_web::error::ErrorBadRequest("Invalid or expired invitation token"))?;

    let password_secret = SecretString::new(params.password.clone().into());

    let password_hash = compute_password_hash(&password_secret)
        .map_err(actix_web::error::ErrorInternalServerError)?;
    let new_user_id = uuid::Uuid::new_v4();

    let insert = sqlx::query!(
        r#"
        INSERT INTO users (user_id, username, password_hash, role)
        VALUES ($1, $2, $3, $4::text::user_role)
        "#,
        new_user_id,
        params.username,
        password_hash.expose_secret(),
        &invitation.role
    )
    .execute(&mut *tx)
    .await
    .map_err(actix_web::error::ErrorInternalServerError)?;

    let consume = sqlx::query!(
        r#"UPDATE user_invitations SET consumed_at = NOW() WHERE id = $1"#,
        invitation.id
    )
    .execute(&mut *tx)
    .await
    .map_err(actix_web::error::ErrorInternalServerError)?;

    match (insert.rows_affected(), consume.rows_affected()) {
        (1, 1) => tx.commit().await.map_err(actix_web::error::ErrorInternalServerError)?,
        _ => {
            tx.rollback().await.map_err(actix_web::error::ErrorInternalServerError)?;
            return Err(actix_web::error::ErrorInternalServerError("Failed to create user record"));
        }
    }

    Ok(HttpResponse::Ok().finish())
}