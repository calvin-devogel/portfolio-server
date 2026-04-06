use actix_web::{HttpResponse, web};
use anyhow::Context;
use secrecy::{ExposeSecret, SecretString};
use sqlx::PgPool;
use uuid::Uuid;

use crate::core::spawn_blocking_with_tracing;
use crate::core::e500;

use super::crypto::compute_password_hash;
use super::models::{RoleUpdate, StoredCredentials, TotpQuery, User, UserId, UserRole};

#[tracing::instrument(name = "Get stored credentials", skip(username, pool))]
pub async fn get_stored_credentials(
    username: &str,
    pool: &PgPool,
) -> Result<Option<StoredCredentials>, anyhow::Error> {
    let row = sqlx::query!(
        r#"
        SELECT user_id, password_hash, totp_enabled, must_change_password, role::TEXT
        FROM users
        WHERE username = $1
        "#,
        username,
    )
    .fetch_optional(pool)
    .await
    .context("Failed to perform a query to retrieve stored credentials.")?
    .map(|row| {
        (
            row.user_id,
            SecretString::new(row.password_hash.into()),
            row.totp_enabled,
            row.must_change_password,
            row.role
                .and_then(|role| role.parse::<UserRole>().ok())
                .unwrap_or(UserRole::User),
        )
    });
    Ok(row)
}

#[tracing::instrument(name = "Get TOTP secret, role, and flags", skip(user_id, pool))]
pub async fn get_totp_secret_role_and_flags(
    user_id: Uuid,
    pool: &PgPool,
) -> Result<Option<TotpQuery>, anyhow::Error> {
    let row = sqlx::query!(
        r#"SELECT totp_secret, role::TEXT, must_change_password FROM users WHERE user_id = $1"#,
        user_id
    )
    .fetch_one(pool)
    .await
    .context("Failed to fetch TOTP secret")?;

    let user_role = row
        .role
        .as_deref()
        .map(|role| role.parse::<UserRole>().unwrap_or(UserRole::User))
        .unwrap_or(UserRole::User);

    Ok(row.totp_secret.map(|secret| TotpQuery {
        secret,
        role: user_role,
        must_change_password: row.must_change_password,
    }))
}

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

pub async fn get_all_users(pool: web::Data<PgPool>) -> Result<HttpResponse, actix_web::Error> {
    let users = sqlx::query_as!(
        User,
        r#"
        SELECT
            user_id::TEXT as "user_id!",
            username,
            role::TEXT as "role!",
            must_change_password
        FROM users"#
    )
    .fetch_all(pool.get_ref())
    .await
    .map_err(actix_web::error::ErrorInternalServerError)?;

    Ok(HttpResponse::Ok().json(users))
}

pub async fn get_username_by_id(
    pool: web::Data<PgPool>,
    user_id: Uuid,
) -> Result<String, actix_web::Error> {
    let user = sqlx::query_as!(
        User,
        r#"
        SELECT
            user_id::TEXT as "user_id!",
            username,
            role::TEXT as "role!",
            must_change_password
        FROM users
        WHERE user_id = $1::UUID
        "#,
        user_id
    )
    .fetch_one(pool.get_ref())
    .await
    .map_err(actix_web::error::ErrorInternalServerError)?;

    Ok(user.username)
}

pub async fn set_user_role(
    pool: web::Data<PgPool>,
    user_id: web::Path<Uuid>,
    new_role: web::Json<RoleUpdate>,
) -> Result<HttpResponse, actix_web::Error> {
    let user_id = user_id.into_inner();
    let new_role = new_role.into_inner();

    sqlx::query!(
        r#"
        UPDATE users
        SET role = $1
        WHERE user_id = $2::UUID
        "#,
        new_role.role as UserRole,
        user_id,
    )
    .execute(pool.get_ref())
    .await
    .map_err(actix_web::error::ErrorInternalServerError)?;

    Ok(HttpResponse::Ok().finish())
}

pub async fn reset_password(
    pool: web::Data<PgPool>,
    user_id: web::Path<Uuid>,
) -> Result<HttpResponse, actix_web::Error> {
    let user_id = user_id.into_inner();

    sqlx::query!(
        r#"
        UPDATE users
        SET must_change_password = TRUE
        WHERE user_id = $1::UUID
        "#,
        user_id,
    )
    .execute(pool.get_ref())
    .await
    .map_err(actix_web::error::ErrorInternalServerError)?;

    Ok(HttpResponse::Ok().finish())
}

#[tracing::instrument(name = "Change password", skip(password, pool))]
/// # Errors
/// errors from anywhere in this function are handled by `anyhow` and passed up the pipeline
pub async fn change_password(
    user_id: Uuid,
    password: SecretString,
    pool: &PgPool,
) -> Result<(), anyhow::Error> {
    let password_hash = spawn_blocking_with_tracing(move || compute_password_hash(&password))
        .await?
        .context("Failed to compute password hash")?;

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
    .context("Failed to change the user's password in the database.")?;
    Ok(())
}
