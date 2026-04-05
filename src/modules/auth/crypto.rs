use sqlx::PgPool;
use secrecy::{ExposeSecret, SecretString};
use anyhow::Context;
use argon2::{
    Algorithm, Argon2, Params, PasswordHash, PasswordHasher, PasswordVerifier, Version,
    password_hash::{SaltString, rand_core::OsRng},
};
use crate::{errors::AuthError, telemetry::spawn_blocking_with_tracing};

use super::models::{Credentials, UserDetails, UserRole};
use super::db::get_stored_credentials;

// wrapper for credential validation that uses the default hash function
// exposed publicly as `validate_credentials` but allows for injecting
// a custom verification function for testing purposes
pub async fn validate_credentials(
    credentials: Credentials,
    pool: &PgPool,
) -> Result<UserDetails, AuthError> {
    validate_credentials_with_verifier(credentials, pool, verify_password_hash).await
}

#[doc(hidden)]
#[tracing::instrument("Validate credentials", skip(credentials, pool, verify_fn))]
pub async fn validate_credentials_with_verifier<F>(
    credentials: Credentials,
    pool: &PgPool,
    verify_fn: F,
) -> Result<UserDetails, AuthError>
where
    F: FnOnce(&SecretString, &SecretString) -> Result<(), AuthError> + Send + 'static, // Trait Bounds!
{
    let mut user_id = None;
    let mut totp_enabled = false;
    let mut must_change_password = false;
    let mut user_role = UserRole::User;

    let expected_password_hash = if let Some((
        stored_user_id,
        stored_password_hash,
        stored_totp_enabled,
        stored_must_change_password,
        stored_user_role,
    )) = get_stored_credentials(&credentials.username, pool).await?
    {
        user_id = Some(stored_user_id);
        totp_enabled = stored_totp_enabled;
        must_change_password = stored_must_change_password;
        user_role = stored_user_role;
        stored_password_hash
    } else {
        // default hash to prevent timing attacks
        SecretString::new(
            "$argon2id$v=19$m=19456,t=2,p=1$\
                gZiV/M1gPc22ElAH/Jh1Hw$\
                CWOrkoo7oJBQ/iyh7uJ0LO2aLEfrHwTWllSAxT0zRno"
                .into(),
        )
    };

    spawn_blocking_with_tracing(move || verify_fn(&expected_password_hash, &credentials.password))
        .await
        .context("Failed to spawn blocking task for password verification.")??;


    // only set to Some if we find stored credentials
    // so even if the default password hash ends up matching (somehow)
    // we never authenticate a non-existent user.
    user_id
        .ok_or_else(|| anyhow::anyhow!("Unknown username"))
        .map_err(AuthError::InvalidCredentials)
        .map(|id| (
            id,
            totp_enabled,
            must_change_password,
            user_role,
        ))
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

pub fn compute_password_hash(password: &SecretString) -> Result<SecretString, anyhow::Error> {
    let salt = SaltString::generate(&mut OsRng);
    // expect is acceptable here because password hashing should never fail
    // if Argon2 is configured and working properly, and we aren't testing Argon2
    // so there's no reason to propogate this error
    let password_hash = Argon2::new(
        Algorithm::Argon2id,
        Version::V0x13,
        Params::new(19456, 2, 1, None).unwrap(),
    )
    .hash_password(password.expose_secret().as_bytes(), &salt)?
    .to_string();
    Ok(SecretString::new(Box::from(password_hash)))
}