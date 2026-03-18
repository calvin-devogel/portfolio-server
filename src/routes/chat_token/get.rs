use actix_web::{HttpResponse, web};
use jsonwebtoken::{EncodingKey, Header, encode};
use secrecy::ExposeSecret;

use crate::{
    authentication::UserId,
    startup::HmacSecret,
    utils::e500,
};

#[derive(serde::Serialize, serde::Deserialize)]
struct ChatClaims {
    subscriber: String,
    expiry: i64,
    issuer: String,
}

pub async fn chat_token(
    user_id: UserId,
    hmac_secret: web::Data<HmacSecret>,
) -> Result<HttpResponse, actix_web::Error> {
    // expires in 60 seconds, once the WebSocket is established, the token
    // is no longer needed
    let expiry = chrono::Utc::now()
        .checked_add_signed(chrono::Duration::seconds(60))
        .ok_or_else(|| e500(anyhow::anyhow!("time overflow")))?
        .timestamp();

    let claims = ChatClaims {
        subscriber: user_id.to_string(),
        expiry,
        issuer: "portfolio-server".to_string(),
    };

    let key = EncodingKey::from_secret(hmac_secret.0.expose_secret().as_bytes());
    let token = encode(&Header::default(), &claims, &key).map_err(e500)?;

    Ok(HttpResponse::Ok().json(serde_json::json!({ "token": token })))
}