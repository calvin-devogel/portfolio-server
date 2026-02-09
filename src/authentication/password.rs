use actix_web::{ResponseError, http::StatusCode};
use anyhow::Context;
use argon2::{
    Algorithm, Argon2, Params, PasswordHash, PasswordHasher, PasswordVerifier, Version,
    password_hash::{SaltString, rand_core::OsRng},
};
use secrecy::{ExposeSecret, SecretString};
use sqlx::PgPool;

use crate::telemetry::spawn_blocking_with_tracing;

#[derive(thiserror::Error, Debug)]
pub enum AuthError {
    #[error("Too many login requests")]
    RateLimitExceeded,
    #[error("Invalid credentials")]
    InvalidCredentials(#[source] anyhow::Error),
    #[error(transparent)]
    UnexpectedError(#[from] anyhow::Error),
}

impl ResponseError for AuthError {
    fn status_code(&self) -> StatusCode {
        match self {
            Self::RateLimitExceeded => StatusCode::TOO_MANY_REQUESTS,
            Self::InvalidCredentials(_) => StatusCode::UNAUTHORIZED,
            Self::UnexpectedError(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

pub struct Credentials {
    pub username: String,
    pub password: SecretString,
}

#[tracing::instrument(name = "Get stored credentials", skip(username, pool))]
async fn get_stored_credentials(
    username: &str,
    pool: &PgPool,
) -> Result<Option<(uuid::Uuid, SecretString)>, anyhow::Error> {
    let row = sqlx::query!(
        r#"
        SELECT user_id, password_hash
        FROM users
        WHERE username = $1
        "#,
        username,
    )
    .fetch_optional(pool)
    .await
    .context("Failed to perform a query to retrieve stored credentials.")?
    .map(|row| (row.user_id, SecretString::new(Box::from(row.password_hash))));
    Ok(row)
}

#[tracing::instrument("Validate credentials", skip(credentials, pool))]
/// # Errors
/// shoots off an `AuthError::InvalidCredentials` if the hash for the provided `credentials` cannot be verified
/// or an `anyhow` error if the `username` doesn't exist in the database
pub async fn validate_credentials(
    credentials: Credentials,
    pool: &PgPool,
) -> Result<uuid::Uuid, AuthError> {
    let mut user_id = None;
    // this is a made-up hash to prevent timing attacks
    // thanks clippy what the fuck is this?
    let expected_password_hash = if let Some((stored_user_id, stored_password_hash)) =
        get_stored_credentials(&credentials.username, pool).await?
    {
        user_id = Some(stored_user_id);
        stored_password_hash
    } else {
        SecretString::new(
            "$argon2id$v=19$m=15000,t=2,p=1$\
                gZiV/M1gPc22ElAH/Jh1Hw$\
                CWOrkoo7oJBQ/iyh7uJ0LO2aLEfrHwTWllSAxT0zRno"
                .into(),
        )
    };

    spawn_blocking_with_tracing(move || {
        verify_password_hash(&expected_password_hash, &credentials.password)
    })
    .await
    .context("Failed to spawn blocking task.")??;

    // only set to Some if we find stored credentials
    // so even if the default password ends up matching (somehow)
    // we never authenticate a non-existent user.
    user_id
        .ok_or_else(|| anyhow::anyhow!("Unknown username."))
        .map_err(AuthError::InvalidCredentials)
}

#[tracing::instrument(
    name = "Verify password hash",
    skip(expected_password_hash, password_candidate)
)]
fn verify_password_hash(
    expected_password_hash: &SecretString,
    password_candidate: &SecretString,
) -> Result<(), AuthError> {
    let expected_password_hash = PasswordHash::new(expected_password_hash.expose_secret())
        .context("Failed to parse hash in PHC string format.")?;

    Argon2::default()
        .verify_password(
            password_candidate.expose_secret().as_bytes(),
            &expected_password_hash,
        )
        .context("Invalid password.")
        .map_err(AuthError::InvalidCredentials)
}

#[tracing::instrument(name = "Change password", skip(password, pool))]
/// # Errors
/// errors from anywheere in this function are handled by `anyhow` and passed up the pipeline
pub async fn change_password(
    user_id: uuid::Uuid,
    password: SecretString,
    pool: &PgPool,
) -> Result<(), anyhow::Error> {
    let password_hash = spawn_blocking_with_tracing(move || compute_password_hash(&password))
        .await?
        .context("Failed to hash password.")?;

    sqlx::query!(
        r#"
        UPDATE users
        SET password_hash = $1
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

fn compute_password_hash(password: &SecretString) -> Result<SecretString, anyhow::Error> {
    let salt = SaltString::generate(&mut OsRng);
    let password_hash = Argon2::new(
        Algorithm::Argon2id,
        Version::V0x13,
        Params::new(15000, 2, 1, None).unwrap(),
    )
    .hash_password(password.expose_secret().as_bytes(), &salt)?
    .to_string();
    Ok(SecretString::new(Box::from(password_hash)))
}
