use secrecy::{ExposeSecret, SecretString};
use sqlx::postgres::PgQueryResult;
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::core::{error::AuthError, spawn_blocking_with_tracing};

use super::crypto::compute_password_hash;
use super::models::{StoredCredentials, TotpQuery, User, UserRole};

#[tracing::instrument(name = "Get stored credentials", skip(username, pool))]
pub async fn get_stored_credentials(
    username: &str,
    pool: &PgPool,
) -> Result<Option<StoredCredentials>, AuthError> {
    let row = sqlx::query!(
        r#"
        SELECT user_id, password_hash, totp_enabled, must_change_password, role as "role: UserRole"
        FROM users
        WHERE username = $1
        "#,
        username,
    )
    .fetch_optional(pool)
    .await
    .map_err(AuthError::Database)?;

    Ok(row.map(|row| {
        (
            row.user_id,
            SecretString::new(row.password_hash.into()),
            row.totp_enabled,
            row.must_change_password,
            row.role.unwrap_or(UserRole::User),
        )
    }))
}

#[tracing::instrument(name = "Get TOTP secret, role, and flags", skip(user_id, pool))]
pub async fn get_totp_secret_role_and_flags(
    user_id: Uuid,
    pool: &PgPool,
) -> Result<Option<TotpQuery>, AuthError> {
    let row = sqlx::query!(
        r#"SELECT totp_secret, role as "role: UserRole", must_change_password FROM users WHERE user_id = $1"#,
        user_id
    )
    .fetch_optional(pool)
    .await
    .map_err(AuthError::Database)?;

    Ok(row.and_then(|r| {
        Some(TotpQuery {
            secret: r.totp_secret?,
            role: r.role.unwrap_or(UserRole::User),
            must_change_password: r.must_change_password,
        })
    }))
}

#[tracing::instrument(name = "Get TOTP status", skip(pool, user_id))]
pub async fn is_totp_enabled(pool: &PgPool, user_id: Uuid) -> Result<bool, AuthError> {
    sqlx::query_scalar!("SELECT totp_enabled FROM users WHERE user_id = $1", user_id)
        .fetch_one(pool)
        .await
        .map_err(AuthError::Database)
}

pub async fn query_users(pool: &PgPool) -> Result<Vec<User>, AuthError> {
    sqlx::query_as!(
        User,
        r#"
        SELECT
            user_id::TEXT as "user_id!",
            username,
            role::TEXT as "role!",
            must_change_password
        FROM users"#
    )
    .fetch_all(pool)
    .await
    .map_err(AuthError::Database)
}

pub async fn get_username_by_id(pool: &PgPool, user_id: Uuid) -> Result<String, AuthError> {
    sqlx::query_scalar!("SELECT username FROM users WHERE user_id = $1", user_id)
        .fetch_one(pool)
        .await
        .map_err(AuthError::Database)
}

pub async fn get_invitation_from_token(
    tx: &mut Transaction<'static, Postgres>,
    token_hash: &str,
) -> Result<Option<(Uuid, UserRole)>, AuthError> {
    let row = sqlx::query!(
        r#"
        SELECT id, email, role as "role: UserRole" FROM user_invitations
        WHERE invitation_token_hash = $1
            AND consumed_at IS NULL
            AND expires_at > NOW()
        "#,
        token_hash,
    )
    .fetch_optional(&mut **tx)
    .await?;

    Ok(row.map(|r| (r.id, r.role)))
}

pub async fn insert_user_invitation(
    transaction: &mut Transaction<'static, Postgres>,
    invitation_id: Uuid,
    email: &str,
    role: UserRole,
    token_hash: &str,
    expires_at: chrono::DateTime<chrono::Utc>,
) -> Result<PgQueryResult, AuthError> {
    sqlx::query!(
        r#"
        INSERT INTO user_invitations (id, email, role, invitation_token_hash, expires_at, created_at)
        VALUES ($1, $2, $3, $4, $5, NOW())
        "#,
        invitation_id,
        email,
        role as UserRole,
        token_hash,
        expires_at,
    )
    .execute(&mut **transaction)
    .await
    .map_err(AuthError::Database)
}

pub async fn insert_user(
    transaction: &mut Transaction<'static, Postgres>,
    user_id: Uuid,
    username: &str,
    password_hash: &str,
    role: UserRole,
) -> Result<PgQueryResult, AuthError> {
    sqlx::query!(
        r#"
        INSERT INTO users (user_id, username, password_hash, role)
        VALUES ($1, $2, $3, $4)
        "#,
        user_id,
        username,
        password_hash,
        role as UserRole,
    )
    .execute(&mut **transaction)
    .await
    .map_err(AuthError::Database)
}

pub async fn consume_invitation(
    transaction: &mut Transaction<'static, Postgres>,
    invitation_id: Uuid,
) -> Result<PgQueryResult, AuthError> {
    sqlx::query!(
        r#"
        UPDATE user_invitations
        SET consumed_at = NOW()
        WHERE id = $1
        "#,
        invitation_id,
    )
    .execute(&mut **transaction)
    .await
    .map_err(AuthError::Database)
}

pub async fn update_user_role(
    pool: &PgPool,
    user_id: Uuid,
    new_role: UserRole,
) -> Result<PgQueryResult, AuthError> {
    sqlx::query!(
        r#"
        UPDATE users
        SET role = $1
        WHERE user_id = $2::UUID
        "#,
        new_role as UserRole,
        user_id,
    )
    .execute(pool)
    .await
    .map_err(AuthError::Database)
}

pub async fn force_password_reset(
    pool: &PgPool,
    user_id: Uuid,
) -> Result<PgQueryResult, AuthError> {
    sqlx::query!(
        r#"
        UPDATE users
        SET must_change_password = TRUE
        WHERE user_id = $1::UUID
        "#,
        user_id,
    )
    .execute(pool)
    .await
    .map_err(AuthError::Database)
}

#[tracing::instrument(name = "Change password", skip(password, pool))]
pub async fn change_password(
    user_id: Uuid,
    password: SecretString,
    pool: &PgPool,
) -> Result<(), AuthError> {
    let password_hash = spawn_blocking_with_tracing(move || compute_password_hash(&password))
        .await
        .expect("Blocking task panicked")
        .expect("Failed to compute password hash");

    sqlx::query!(
        r#"
        UPDATE users
        SET password_hash = $1, must_change_password = FALSE
        WHERE user_id = $2
        "#,
        password_hash.expose_secret(),
        user_id
    )
    .execute(pool)
    .await
    .map_err(AuthError::Database)?;

    Ok(())
}
