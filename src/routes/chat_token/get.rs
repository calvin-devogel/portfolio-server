use actix_web::{HttpResponse, web};
use anyhow::Context;
use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
use secrecy::ExposeSecret;
use sqlx::PgPool;

use crate::{authentication::UserId, startup::JwtPrivateKey, utils::e500};

// non-semantic names dangit!
// SignalR maps sub to ClaimTypes.NameIdentifier
// sub -> who (UUID)
// exp -> expiry (60 seconds)
// iss -> issuer ("portfolio-server")
#[derive(serde::Serialize, serde::Deserialize)]
struct ChatClaims {
    name: String,
    sub: String,
    exp: i64,
    iss: String,
}

#[tracing::instrument("Get chat token", skip(jwt_key, pool))]
pub async fn chat_token(
    user_id: UserId,
    jwt_key: web::Data<JwtPrivateKey>,
    pool: web::Data<PgPool>,
) -> Result<HttpResponse, actix_web::Error> {
    // expires in 60 seconds, once the WebSocket is established, the token
    // is no longer needed
    let exp = chrono::Utc::now()
        .checked_add_signed(chrono::Duration::seconds(60))
        .ok_or_else(|| e500(anyhow::anyhow!("time overflow")))?
        .timestamp();

    let user_record = sqlx::query!("SELECT username FROM users WHERE user_id = $1", *user_id)
        .fetch_one(pool.as_ref())
        .await
        .context("Failed to fetch username")
        .map_err(e500)?;

    let claims = ChatClaims {
        name: user_record.username,
        sub: user_id.to_string(),
        exp,
        iss: "portfolio-server".to_string(),
    };

    let pem = jwt_key.0.expose_secret();
    tracing::info!(pem_len = pem.len(), "Attempting to parse JWT private key");

    let key = EncodingKey::from_ec_pem(pem.as_bytes()).map_err(|e| {
        tracing::error!(error = ?e, "EncodingKey::from_ec_pem failed");
        e500(e)
    })?;

    let token = encode(&Header::new(Algorithm::ES256), &claims, &key).map_err(e500)?;

    Ok(HttpResponse::Ok().json(serde_json::json!({ "token": token })))
}
