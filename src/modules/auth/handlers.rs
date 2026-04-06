use crate::errors::AuthError;
use actix_web::{HttpRequest, HttpResponse, dev::ConnectionInfo, web};
use anyhow::Context;
use rand::{RngExt, distr::Alphanumeric};
use secrecy::{ExposeSecret, SecretString};
use sqlx::PgPool;
use totp_rs::Secret;

use super::crypto::{
    build_totp, compute_password_hash, encrypt, sha256_hash, totp_from_encrypted,
    validate_credentials,
};
use super::db::{change_password, get_totp_secret_role_and_flags, get_username_by_id};
use super::models::{
    AcceptInvitationParams, ChangePasswordBody, CreateUser, Credentials, DisableTotpRequest,
    TotpEncryptionKey, TotpQuery, TotpRequest, UserId,
};
use super::session::TypedSession;

use crate::api::idempotency::execute_idempotent;
use crate::api::startup::ApplicationBaseUrl;

use crate::core::e500;

#[allow(clippy::missing_errors_doc)]
#[allow(clippy::future_not_send)]
#[tracing::instrument(
    skip(pool, session),
    fields(username=tracing::field::Empty, user_id=tracing::field::Empty)
)]
pub async fn login(
    _conn: ConnectionInfo,
    request: web::Form<Credentials>,
    pool: web::Data<PgPool>,
    session: TypedSession,
) -> Result<HttpResponse, AuthError> {
    let (user_id, totp_enabled, must_change_password, user_role) =
        validate_credentials(request.0, &pool).await?;

    tracing::Span::current().record("user_id", tracing::field::display(&user_id));
    session.renew();

    if totp_enabled {
        session.clear_user_id();
        session
            .insert_mfa_pending_user_id(user_id)
            .map_err(|e| AuthError::UnexpectedError(e.into()))?;

        Ok(HttpResponse::Accepted().json(serde_json::json!({ "mfa_required": true })))
    } else {
        session
            .insert_user_id(user_id)
            .map_err(|e| AuthError::UnexpectedError(e.into()))?;
        session
            .insert_user_role(user_role)
            .map_err(|e| AuthError::UnexpectedError(e.into()))?;

        Ok(ok_must_change(must_change_password))
    }
}

#[allow(clippy::missing_errors_doc)]
#[allow(clippy::future_not_send)]
pub async fn logout(session: TypedSession) -> Result<HttpResponse, actix_web::Error> {
    session.log_out();
    Ok(HttpResponse::Ok().finish())
}

// I feel like this should be extended
#[allow(clippy::future_not_send)]
#[tracing::instrument(name = "Check if authenticated", skip(session))]
pub async fn check_auth(session: TypedSession) -> HttpResponse {
    match session.get_user_id() {
        Ok(Some(_)) => {
            // renew session on each check_auth to extend TTL
            session.renew();
            let user_role = session.get_user_role();
            match user_role {
                Ok(Some(role)) => HttpResponse::Ok().json(role.to_string()),
                _ => HttpResponse::Unauthorized().finish(),
            }
        }
        _ => HttpResponse::Unauthorized().finish(),
    }
}

#[allow(clippy::future_not_send)]
#[tracing::instrument(
    name = "Verify TOTP code",
    skip(pool, session, request, encryption_key)
)]
pub async fn verify_totp(
    request: web::Json<TotpRequest>,
    pool: web::Data<PgPool>,
    session: TypedSession,
    encryption_key: web::Data<TotpEncryptionKey>,
) -> Result<HttpResponse, actix_web::Error> {
    let user_id = session
        .get_mfa_pending_user_id()
        .map_err(e500)?
        .ok_or_else(|| actix_web::error::ErrorUnauthorized("No MFA session in progress"))?;

    let TotpQuery {
        secret: encrypted,
        role: user_role,
        must_change_password,
    } = get_totp_secret_role_and_flags(user_id, &pool)
        .await
        .map_err(e500)?
        .ok_or_else(|| {
            actix_web::error::ErrorUnauthorized(format!("TOTP not configured for user: {user_id}"))
        })?;

    let totp = totp_from_encrypted(&encryption_key.0, &encrypted, user_id).map_err(e500)?;

    if totp.check_current(&request.code).unwrap_or(false) {
        session.clear_mfa_pending();
        session.insert_user_id(user_id).map_err(e500)?;
        session.insert_user_role(user_role).map_err(e500)?;
        Ok(ok_must_change(must_change_password))
    } else {
        Ok(HttpResponse::Unauthorized().finish())
    }
}

#[tracing::instrument(name = "TOTP confirm", skip(pool, user_id, request, encryption_key))]
pub async fn totp_confirm(
    request: web::Json<TotpRequest>,
    pool: web::Data<PgPool>,
    user_id: UserId,
    encryption_key: web::Data<TotpEncryptionKey>,
) -> Result<HttpResponse, actix_web::Error> {
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

    let encrypted = row
        .totp_secret
        .ok_or_else(|| actix_web::error::ErrorBadRequest("No TOTP setup in progres"))?;

    let totp = totp_from_encrypted(&encryption_key.0, &encrypted, user_id.0).map_err(e500)?;

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

#[tracing::instrument(name = "TOTP disable", skip(pool, user_id, request))]
pub async fn totp_disable(
    request: web::Json<DisableTotpRequest>,
    pool: web::Data<PgPool>,
    user_id: UserId,
) -> Result<HttpResponse, actix_web::Error> {
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

#[tracing::instrument(name = "TOTP setup", skip(pool, user_id, encryption_key))]
pub async fn totp_setup(
    pool: web::Data<PgPool>,
    user_id: UserId,
    encryption_key: web::Data<TotpEncryptionKey>,
) -> Result<HttpResponse, actix_web::Error> {
    let status = sqlx::query!(
        "SELECT totp_enabled FROM users WHERE user_id = $1",
        *user_id
    )
    .fetch_one(pool.as_ref())
    .await
    .context("Failed to get totp status")
    .map_err(e500)?;

    if status.totp_enabled {
        return Ok(HttpResponse::Conflict().finish());
    }

    // generate a secret and encode
    let secret = Secret::generate_secret();
    let secret_b32 = secret.to_encoded().to_string();
    let encrypted = encrypt(&encryption_key.0, secret_b32.as_bytes())
        .context("Failed to encrypt TOTP secret")
        .map_err(e500)?;

    sqlx::query!(
        "UPDATE users SET totp_secret = $1 WHERE user_id = $2",
        encrypted as Vec<u8>,
        *user_id,
    )
    .execute(pool.as_ref())
    .await
    .context("Failed to store pending TOTP secret")
    .map_err(e500)?;

    let totp = build_totp(secret_b32, *user_id).map_err(e500)?;

    let otpauth_uri = totp.get_url();

    Ok(HttpResponse::Ok().json(serde_json::json!({ "otpauth_uri": otpauth_uri })))
}

#[tracing::instrument(name = "Create user invitation", skip_all)]
pub async fn create_user(
    new_user: web::Json<CreateUser>,
    pool: web::Data<PgPool>,
    request: HttpRequest,
    user_id: UserId,
    base_url: web::Data<ApplicationBaseUrl>,
) -> Result<HttpResponse, actix_web::Error> {
    let user_to_create = new_user.into_inner();
    user_to_create.validate()?;

    execute_idempotent(&request, &pool, Some(*user_id), move |tx| {
        Box::pin(async move { process_create_new_user(tx, user_to_create, &base_url.0).await })
    })
    .await
}

#[allow(clippy::future_not_send)]
async fn process_create_new_user(
    transaction: &mut sqlx::Transaction<'static, sqlx::Postgres>,
    new_user: CreateUser,
    base_url: &str,
) -> Result<HttpResponse, actix_web::Error> {
    // random raw token
    let raw_token: String = rand::rng()
        .sample_iter(&Alphanumeric)
        .take(32)
        .map(char::from)
        .collect();

    // hash the token
    let token_hash = sha256_hash(&raw_token);

    let expires_at = chrono::Utc::now() + chrono::Duration::hours(24);
    let invitation_id = uuid::Uuid::new_v4();

    sqlx::query!(
        r#"
        INSERT INTO user_invitations (id, email, role, invitation_token_hash, expires_at, created_at)
        VALUES ($1, $2, $3, $4, $5, NOW())
        "#,
        invitation_id,
        new_user.email,
        "user".to_string(), // default to "user" role for invitations, admin can change later
        token_hash,
        expires_at,
    )
    .execute(transaction.as_mut())
    .await
    .map_err(|e| {
        actix_web::error::ErrorInternalServerError(format!("Failed to create user invitation: {}", e))
    })?;

    // sidestepping an email service, don't really wanna implement that for this project
    let response_data = serde_json::json!({
        "success": true,
        "message": "Invitation created successfully.",
        "link": format!("{}/invitation/accept?token={}", base_url, raw_token)
    });

    Ok(HttpResponse::Ok().json(response_data))
}

pub async fn update_user_password(
    pool: web::Data<PgPool>,
    body: web::Json<ChangePasswordBody>,
    user_id: UserId,
) -> Result<HttpResponse, AuthError> {
    let body = body.into_inner();

    // First, we need to validate the current password
    let credentials = Credentials {
        username: get_username_by_id(pool.clone(), *user_id)
            .await
            .map_err(|e| anyhow::anyhow!(e.to_string()))
            .context("Failed to retrieve username for user ID.")
            .map_err(AuthError::UnexpectedError)?,
        password: body.current_password.clone(),
    };

    validate_credentials(credentials, &pool).await?;

    // If validation succeeds, we can proceed to change the password
    change_password(*user_id, body.new_password, pool.get_ref())
        .await
        .context("Failed to change password.")
        .map_err(AuthError::UnexpectedError)?;

    Ok(HttpResponse::Accepted().finish())
}
#[tracing::instrument(name = "Accept user invitation", skip_all)]
pub async fn accept_invitation(
    params: web::Json<AcceptInvitationParams>,
    pool: web::Data<PgPool>,
) -> Result<HttpResponse, actix_web::Error> {
    let token_hash = sha256_hash(&params.token);

    // don't need idempotency here since invitation accepts are one-time
    let mut tx = pool
        .begin()
        .await
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
        (1, 1) => tx
            .commit()
            .await
            .map_err(actix_web::error::ErrorInternalServerError)?,
        _ => {
            tx.rollback()
                .await
                .map_err(actix_web::error::ErrorInternalServerError)?;
            return Err(actix_web::error::ErrorInternalServerError(
                "Failed to create user record",
            ));
        }
    }

    Ok(HttpResponse::Ok().finish())
}

fn ok_must_change(must_change_password: bool) -> HttpResponse {
    if must_change_password {
        HttpResponse::Ok().json(serde_json::json!({ "must_change_password": true }))
    } else {
        HttpResponse::Ok().finish()
    }
}
