use secrecy::{ExposeSecret, SecretString};
use sqlx::PgPool;
use sqlx::postgres::PgQueryResult;
use uuid::Uuid;

use crate::core::spawn_blocking_with_tracing;

use super::crypto::compute_password_hash;
use super::models::{StoredCredentials, TotpQuery, User, UserRole};

#[tracing::instrument(name = "Get stored credentials", skip(username, pool))]
pub async fn get_stored_credentials(
    username: &str,
    pool: &PgPool,
) -> Result<Option<StoredCredentials>, sqlx::Error> {
    let row = sqlx::query!(
        r#"
        SELECT user_id, password_hash, totp_enabled, must_change_password, role as "role: UserRole"
        FROM users
        WHERE username = $1
        "#,
        username,
    )
    .fetch_optional(pool)
    .await?;

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
) -> Result<Option<TotpQuery>, sqlx::Error> {
    let row = sqlx::query!(
        r#"SELECT totp_secret, role as "role: UserRole", must_change_password FROM users WHERE user_id = $1"#,
        user_id
    )
    .fetch_optional(pool)
    .await?;

    Ok(row.and_then(|r| {
        Some(TotpQuery {
            secret: r.totp_secret?,
            role: r.role.unwrap_or(UserRole::User),
            must_change_password: r.must_change_password,
        })
    }))
}

#[tracing::instrument(name = "Get TOTP status", skip(pool, user_id))]
pub async fn is_totp_enabled(pool: &PgPool, user_id: Uuid) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar!("SELECT totp_enabled FROM users WHERE user_id = $1", user_id)
        .fetch_one(pool)
        .await
}

pub async fn query_users(pool: &PgPool) -> Result<Vec<User>, sqlx::Error> {
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
}

pub async fn get_username_by_id(pool: &PgPool, user_id: Uuid) -> Result<String, sqlx::Error> {
    sqlx::query_scalar!("SELECT username FROM users WHERE user_id = $1", user_id)
        .fetch_one(pool)
        .await
}

pub async fn update_user_role(
    pool: &PgPool,
    user_id: Uuid,
    new_role: UserRole,
) -> Result<PgQueryResult, sqlx::Error> {
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
}

pub async fn force_password_reset(
    pool: &PgPool,
    user_id: Uuid,
) -> Result<PgQueryResult, sqlx::Error> {
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
}

#[tracing::instrument(name = "Change password", skip(password, pool))]
/// # Errors
/// errors from anywhere in this function are handled by `anyhow` and passed up the pipeline
#[tracing::instrument(name = "Change password", skip(password, pool))]
pub async fn change_password(
    user_id: Uuid,
    password: SecretString,
    pool: &PgPool,
) -> Result<(), sqlx::Error> {
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
    .await?;

    Ok(())
}
