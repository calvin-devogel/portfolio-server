use anyhow::Context;
use argon2::{
    Algorithm, Argon2, Params, PasswordHash, PasswordHasher, PasswordVerifier, Version,
    password_hash::{SaltString, rand_core::OsRng},
};
use secrecy::{ExposeSecret, SecretString};
use sqlx::PgPool;

use crate::errors::AuthError;
use crate::telemetry::spawn_blocking_with_tracing;

pub struct Credentials {
    pub username: String,
    pub password: SecretString,
}

#[tracing::instrument(name = "Get stored credentials", skip(username, pool))]
async fn get_stored_credentials(
    username: &str,
    pool: &PgPool,
) -> Result<Option<(uuid::Uuid, SecretString, bool)>, anyhow::Error> {
    let row = sqlx::query!(
        r#"
        SELECT user_id, password_hash, totp_enabled
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
        )
    });
    Ok(row)
}

pub async fn validate_credentials(
    credentials: Credentials,
    pool: &PgPool,
) -> Result<(uuid::Uuid, bool), AuthError> {
    validate_credentials_with_verifier(credentials, pool, verify_password_hash).await
}

#[doc(hidden)]
#[tracing::instrument("Validate credentials", skip(credentials, pool, verify_fn))]
/// # Errors
/// shoots off an `AuthError::InvalidCredentials` if the hash for the provided `credentials` cannot be verified
/// or an `anyhow` error if the `username` doesn't exist in the database
pub async fn validate_credentials_with_verifier<F>(
    credentials: Credentials,
    pool: &PgPool,
    verify_fn: F,
) -> Result<(uuid::Uuid, bool), AuthError>
where
    F: FnOnce(&SecretString, &SecretString) -> Result<(), AuthError> + Send + 'static, // Trait Bounds!
{
    let mut user_id = None;
    let mut totp_enabled = false;
    // this is a made-up hash to prevent timing attacks
    // thanks clippy what the fuck is this?
    let expected_password_hash =
        if let Some((stored_user_id, stored_password_hash, stored_totp_enabled)) =
            get_stored_credentials(&credentials.username, pool).await?
        {
            user_id = Some(stored_user_id);
            totp_enabled = stored_totp_enabled;
            stored_password_hash
        } else {
            SecretString::new(
                "$argon2id$v=19$m=19456,t=2,p=1$\
                gZiV/M1gPc22ElAH/Jh1Hw$\
                CWOrkoo7oJBQ/iyh7uJ0LO2aLEfrHwTWllSAxT0zRno"
                    .into(),
            )
        };

    spawn_blocking_with_tracing(move || verify_fn(&expected_password_hash, &credentials.password))
        .await
        .context("Failed to spawn blocking task.")??;

    // only set to Some if we find stored credentials
    // so even if the default password ends up matching (somehow)
    // we never authenticate a non-existent user.
    user_id
        .ok_or_else(|| anyhow::anyhow!("Unknown username"))
        .map_err(AuthError::InvalidCredentials)
        .map(|id| (id, totp_enabled))
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
/// errors from anywhere in this function are handled by `anyhow` and passed up the pipeline
pub async fn change_password(
    user_id: uuid::Uuid,
    password: SecretString,
    pool: &PgPool,
) -> Result<(), anyhow::Error> {
    let password_hash = spawn_blocking_with_tracing(move || compute_password_hash(&password))
        .await?;

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

fn compute_password_hash(password: &SecretString) -> SecretString {
    let salt = SaltString::generate(&mut OsRng);
    // expect is acceptable here because password hashing should never fail
    // if Argon2 is configured and working properly, and we aren't testing Argon2
    // so there's no reason to propogate this error
    let password_hash = Argon2::new(
        Algorithm::Argon2id,
        Version::V0x13,
        Params::new(19456, 2, 1, None).unwrap(),
    )
    .hash_password(password.expose_secret().as_bytes(), &salt)
    .expect("Password hashing failed")
    .to_string();
    SecretString::new(Box::from(password_hash))
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn verify_password_hash_gives_correct_context() {
        let fake_expected_password_hash = SecretString::new("improperly_formatted_hash".into());
        let fake_password_candidate = SecretString::new("fake_candidate".into());

        let result = verify_password_hash(&fake_expected_password_hash, &fake_password_candidate);

        let e = result.unwrap_err();

        assert!(
            e.to_string()
                .contains("Failed to parse hash in PHC string format.")
        );
    }
}
