use actix_web::{HttpResponse, web};
use anyhow::Context;
use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
use secrecy::ExposeSecret;
use sqlx::PgPool;
use uuid::Uuid;

use crate::{api::startup::JwtPrivateKey, modules::auth::UserId};

use crate::modules::auth::get_username_by_id;

use super::models::ChatClaims;

#[tracing::instrument("Get chat token", skip(jwt_key, pool))]
pub async fn chat_token(
    user_id: UserId,
    jwt_key: web::Data<JwtPrivateKey>,
    pool: web::Data<PgPool>,
) -> Result<HttpResponse, actix_web::Error> {
    // fetch username
    let username = get_username_by_id(pool.as_ref(), *user_id)
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;

    // generate via helper
    let token = generate_chat_jwt(&username, *user_id, &jwt_key.0)
        .map_err(actix_web::error::ErrorInternalServerError)?;

    Ok(HttpResponse::Ok().json(serde_json::json!({ "token": token })))
}

fn generate_chat_jwt(
    username: &str,
    user_id: Uuid,
    jwt_key: &secrecy::SecretString,
) -> Result<String, anyhow::Error> {
    let exp = chrono::Utc::now()
        .checked_add_signed(chrono::Duration::seconds(60))
        .ok_or_else(|| anyhow::anyhow!("time overflow"))?
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

    let key =
        EncodingKey::from_ec_pem(pem.as_bytes()).context("EncodingKey::from_ec_pem failed")?;

    encode(&Header::new(Algorithm::ES256), &claims, &key).context("JWT encode failed")
}
