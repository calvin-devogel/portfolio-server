use sqlx::PgPool;
use secrecy::SecretString;
use anyhow::Context;

use super::models::{StoredCredentials, UserRole};

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