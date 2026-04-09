use actix_web::{HttpRequest, HttpResponse, web};
use anyhow::Context;
use rand::{RngExt, distr::Alphanumeric};
use secrecy::{ExposeSecret, SecretString};
use sqlx::PgPool;
use totp_rs::Secret;
use uuid::Uuid;

use super::crypto::{
    build_totp, compute_password_hash, encrypt, sha256_hash, totp_from_encrypted,
    validate_credentials,
};
use super::db::{
    change_password, consume_invitation, force_password_reset, get_invitation_from_token,
    get_totp_secret_role_and_flags, get_username_by_id, insert_user, insert_user_invitation,
    is_totp_enabled, query_users, update_user_role,
};
use super::models::{
    AcceptInvitationParams, ChangePasswordBody, CreateUser, Credentials, DisableTotpRequest,
    RoleUpdate, TotpEncryptionKey, TotpQuery, TotpRequest, UserId, UserRole,
};
use super::session::TypedSession;

use crate::api::idempotency::execute_idempotent;
use crate::api::startup::ApplicationBaseUrl;

use crate::core::error::AuthError;

// Basic Auth Handlers (checks, login, logout)
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
    skip(pool, session),
    fields(username=tracing::field::Empty, user_id=tracing::field::Empty)
)]
pub async fn login(
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
            .map_err(|e| AuthError::Unexpected(e.into()))?;

        Ok(HttpResponse::Accepted().json(serde_json::json!({ "mfa_required": true })))
    } else {
        session
            .insert_user_id(user_id)
            .map_err(|e| AuthError::Unexpected(e.into()))?;
        session
            .insert_user_role(user_role)
            .map_err(|e| AuthError::Unexpected(e.into()))?;

        Ok(ok_must_change(must_change_password))
    }
}

#[allow(clippy::future_not_send)]
pub async fn logout(session: TypedSession) -> Result<HttpResponse, AuthError> {
    session.log_out();
    Ok(HttpResponse::Ok().finish())
}

// TOTP Handlers

#[tracing::instrument(name = "TOTP setup", skip(pool, user_id, encryption_key))]
pub async fn totp_setup(
    pool: web::Data<PgPool>,
    user_id: UserId,
    encryption_key: web::Data<TotpEncryptionKey>,
) -> Result<HttpResponse, AuthError> {
    let totp_enabled = is_totp_enabled(pool.as_ref(), *user_id)
        .await
        .map_err(|e| AuthError::Unexpected(e.into()))?;

    if totp_enabled {
        return Err(AuthError::Conflict(
            "TOTP is already enabled for this user".into(),
        ));
    }

    // generate a secret and encode
    let secret = Secret::generate_secret();
    let secret_b32 = secret.to_encoded().to_string();
    let encrypted = encrypt(&encryption_key.0, secret_b32.as_bytes())
        .context("Failed to encrypt TOTP secret")
        .map_err(AuthError::Unexpected)?;

    sqlx::query!(
        "UPDATE users SET totp_secret = $1 WHERE user_id = $2",
        encrypted as Vec<u8>,
        *user_id,
    )
    .execute(pool.as_ref())
    .await
    .context("Failed to store pending TOTP secret")
    .map_err(AuthError::Unexpected)?;

    let totp = build_totp(secret_b32, *user_id).map_err(AuthError::Unexpected)?;

    let otpauth_uri = totp.get_url();

    Ok(HttpResponse::Ok().json(serde_json::json!({ "otpauth_uri": otpauth_uri })))
}

#[tracing::instrument(name = "TOTP confirm", skip(pool, user_id, request, encryption_key))]
pub async fn totp_confirm(
    request: web::Json<TotpRequest>,
    pool: web::Data<PgPool>,
    user_id: UserId,
    encryption_key: web::Data<TotpEncryptionKey>,
) -> Result<HttpResponse, AuthError> {
    let row = sqlx::query!(
        "SELECT totp_secret, totp_enabled FROM users WHERE user_id =  $1",
        *user_id,
    )
    .fetch_one(pool.as_ref())
    .await
    .context("Failed to fetch TOTP state")
    .map_err(AuthError::Unexpected)?;

    // reject if already enabled or no secret
    if row.totp_enabled {
        return Err(AuthError::Conflict(
            "TOTP is already enabled for this user".into(),
        ));
    }

    let encrypted = row
        .totp_secret
        .ok_or_else(|| AuthError::BadRequest("No TOTP setup in progress".into()))?;

    let totp = totp_from_encrypted(&encryption_key.0, &encrypted, user_id.0)
        .map_err(AuthError::Unexpected)?;

    if !totp.check_current(&request.code).unwrap_or(false) {
        return Err(AuthError::Unauthorized(
            "Invalid TOTP verification code".into(),
        ));
    }

    sqlx::query!(
        "UPDATE users SET totp_enabled = TRUE WHERE user_id = $1",
        *user_id
    )
    .execute(pool.as_ref())
    .await
    .context("Failed to enable TOTP")
    .map_err(AuthError::Unexpected)?;

    Ok(HttpResponse::Ok().finish())
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
) -> Result<HttpResponse, AuthError> {
    let user_id = session
        .get_mfa_pending_user_id()
        .map_err(|e| AuthError::Unexpected(e.into()))?
        .ok_or_else(|| AuthError::Unauthorized("No MFA session in progress".into()))?;

    let TotpQuery {
        secret: encrypted,
        role: user_role,
        must_change_password,
    } = get_totp_secret_role_and_flags(user_id, &pool)
        .await?
        .ok_or_else(|| {
            AuthError::Unauthorized(format!("TOTP not configured for user: {user_id}"))
        })?;

    let totp = totp_from_encrypted(&encryption_key.0, &encrypted, user_id)
        .map_err(AuthError::Unexpected)?;

    if totp.check_current(&request.code).unwrap_or(false) {
        session.clear_mfa_pending();
        session.insert_user_id(user_id)?;
        session.insert_user_role(user_role)?;
        Ok(ok_must_change(must_change_password))
    } else {
        Err(AuthError::Unauthorized(
            "Invalid TOTP verification code".into(),
        ))
    }
}

#[tracing::instrument(name = "TOTP disable", skip(pool, user_id, request))]
pub async fn totp_disable(
    request: web::Json<DisableTotpRequest>,
    pool: web::Data<PgPool>,
    user_id: UserId,
) -> Result<HttpResponse, AuthError> {
    let username = sqlx::query_scalar!("SELECT username FROM users WHERE user_id = $1", *user_id,)
        .fetch_one(pool.as_ref())
        .await
        .context("Failed to fetch username")
        .map_err(AuthError::Unexpected)?;

    // revalidate before allowing removal
    let credentials = Credentials {
        username,
        password: request.into_inner().password,
    };

    validate_credentials(credentials, &pool)
        .await
        .map_err(|_| AuthError::Unauthorized("Invalid password".into()))?;

    sqlx::query!(
        "UPDATE users SET totp_secret = NULL, totp_enabled = FALSE WHERE user_id = $1",
        *user_id,
    )
    .execute(pool.as_ref())
    .await
    .context("Failed to disable TOTP")
    .map_err(AuthError::Unexpected)?;

    Ok(HttpResponse::Ok().finish())
}

#[tracing::instrument(name = "TOTP status", skip(pool, user_id))]
pub async fn totp_status(
    pool: web::Data<PgPool>,
    user_id: UserId,
) -> Result<HttpResponse, AuthError> {
    let enabled = is_totp_enabled(pool.as_ref(), user_id.0)
        .await
        .context("Failed to query TOTP status")
        .map_err(AuthError::Unexpected)?;

    Ok(HttpResponse::Ok().json(serde_json::json!({ "totp_enabled": enabled })))
}

// User Management

pub async fn get_all_users(pool: web::Data<PgPool>) -> Result<HttpResponse, AuthError> {
    let users = query_users(pool.as_ref())
        .await
        .context("Failed to query users from the database.")
        .map_err(AuthError::Unexpected)?;

    Ok(HttpResponse::Ok().json(users))
}

#[tracing::instrument(name = "Create user invitation", skip_all)]
#[allow(clippy::future_not_send)]
pub async fn create_user(
    new_user: web::Json<CreateUser>,
    pool: web::Data<PgPool>,
    request: HttpRequest,
    user_id: UserId,
    base_url: web::Data<ApplicationBaseUrl>,
) -> Result<HttpResponse, AuthError> {
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
) -> Result<HttpResponse, AuthError> {
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

    insert_user_invitation(
        transaction,
        invitation_id,
        &new_user.email,
        UserRole::User,
        &token_hash,
        expires_at,
    )
    .await?;

    // sidestepping an email service, don't really wanna implement that for this project
    let response_data = serde_json::json!({
        "success": true,
        "message": "Invitation created successfully.",
        "link": format!("{}/invitation/accept?token={}", base_url, raw_token)
    });

    Ok(HttpResponse::Ok().json(response_data))
}

#[tracing::instrument(name = "Accept user invitation", skip_all)]
pub async fn accept_invitation(
    params: web::Json<AcceptInvitationParams>,
    pool: web::Data<PgPool>,
) -> Result<HttpResponse, AuthError> {
    let token_hash = sha256_hash(&params.token);

    // don't need idempotency here since invitation accepts are one-time
    let mut tx = pool
        .begin()
        .await
        .map_err(|e| AuthError::Unexpected(e.into()))?;

    let invitation = get_invitation_from_token(&mut tx, &token_hash)
        .await?
        .ok_or_else(|| AuthError::Unauthorized("Invalid or expired invitation token".into()))?;

    let password_secret = SecretString::new(params.password.clone().into());

    let password_hash = compute_password_hash(&password_secret).map_err(AuthError::Unexpected)?;

    let new_user_id = uuid::Uuid::new_v4();

    let insert_result = insert_user(
        &mut tx,
        new_user_id,
        &params.username,
        password_hash.expose_secret(),
        invitation.1,
    )
    .await?;

    let consume_result = consume_invitation(&mut tx, invitation.0).await?;
    if (
        insert_result.rows_affected(),
        consume_result.rows_affected(),
    ) == (1, 1)
    {
        tx.commit().await?;
    } else {
        tx.rollback().await?;
        return Err(AuthError::Unexpected(anyhow::anyhow!(
            "Failed to atomically create user and consume invitation"
        )));
    }

    Ok(HttpResponse::Ok().finish())
}

pub async fn update_user_password(
    pool: web::Data<PgPool>,
    body: web::Json<ChangePasswordBody>,
    user_id: UserId,
) -> Result<HttpResponse, AuthError> {
    let body = body.into_inner();

    let credentials = Credentials {
        // Just borrow the pool with .as_ref(), pass auth error up cleanly
        username: get_username_by_id(pool.as_ref(), *user_id).await?,
        password: body.current_password.clone(),
    };

    validate_credentials(credentials, &pool).await?;

    change_password(*user_id, body.new_password, pool.as_ref()).await?;

    Ok(HttpResponse::Accepted().finish())
}

pub async fn set_user_role(
    pool: web::Data<PgPool>,
    user_id: web::Path<Uuid>,
    new_role: web::Json<RoleUpdate>,
) -> Result<HttpResponse, AuthError> {
    let params = new_role.into_inner();

    update_user_role(pool.get_ref(), *user_id, params.role).await?;

    Ok(HttpResponse::Ok().finish())
}

pub async fn reset_password(
    pool: web::Data<PgPool>,
    user_id: web::Path<Uuid>,
) -> Result<HttpResponse, AuthError> {
    force_password_reset(pool.get_ref(), *user_id).await?;

    Ok(HttpResponse::Ok().finish())
}

// Helpers

fn ok_must_change(must_change_password: bool) -> HttpResponse {
    if must_change_password {
        HttpResponse::Ok().json(serde_json::json!({ "must_change_password": true }))
    } else {
        HttpResponse::Ok().finish()
    }
}
