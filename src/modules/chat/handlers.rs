use actix_web::{HttpResponse, web};
use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
use secrecy::ExposeSecret;
use sqlx::PgPool;
use uuid::Uuid;

use crate::{
    api::startup::JwtPrivateKey,
    core::error::{AuthError, ChatError},
    modules::auth::{UserId, get_username_by_id},
};

use super::models::ChatClaims;

#[tracing::instrument("Get chat token", skip(jwt_key, pool))]
pub async fn chat_token(
    user_id: UserId,
    jwt_key: web::Data<JwtPrivateKey>,
    pool: web::Data<PgPool>,
) -> Result<HttpResponse, ChatError> {
    // fetch username
    let username = get_username_by_id(pool.as_ref(), *user_id)
        .await
        .map_err(|e| match e {
            AuthError::Database(sqlx::Error::RowNotFound) => ChatError::UserNotFound(e),
            _ => ChatError::Unexpected(e.into()),
        })?;

    // generate via helper
    let token = generate_chat_jwt(&username, *user_id, &jwt_key.0)
        .map_err(|e| ChatError::Unexpected(e.into()))?;

    Ok(HttpResponse::Ok().json(serde_json::json!({ "token": token })))
}

fn generate_chat_jwt(
    username: &str,
    user_id: Uuid,
    jwt_key: &secrecy::SecretString,
) -> Result<String, ChatError> {
    let exp = chrono::Utc::now()
        .checked_add_signed(chrono::Duration::seconds(60))
        .ok_or_else(|| ChatError::JwtGeneration("time overflow".to_string()))?
        .timestamp();

    let claims = ChatClaims {
        name: username.to_string(),
        sub: user_id.to_string(),
        exp,
        iss: "portfolio-server".to_string(),
        aud: "portfolio-chat".to_string(),
    };

    let pem = jwt_key.expose_secret();
    tracing::info!(pem_len = pem.len(), "Attempting to parse JWT private key");

    let key = EncodingKey::from_ec_pem(pem.as_bytes())
        .map_err(|e| ChatError::JwtGeneration(e.to_string()))?;

    encode(&Header::new(Algorithm::ES256), &claims, &key)
        .map_err(|e| ChatError::JwtGeneration(e.to_string()))
}
