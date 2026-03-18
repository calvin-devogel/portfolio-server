use actix_web::{HttpResponse, web};
use jsonwebtoken::{EncodingKey, Header, encode};
use secrecy::ExposeSecret;

use crate::{authentication::UserId, startup::HmacSecret, utils::e500};

// non-semantic names dangit!
// SignalR maps sub to ClaimTypes.NameIdentifier
// sub -> who (UUID)
// exp -> expiry (60 seconds)
// iss -> issuer ("portfolio-server")
#[derive(serde::Serialize, serde::Deserialize)]
struct ChatClaims {
    sub: String,
    exp: i64,
    iss: String,
}

pub async fn chat_token(
    user_id: UserId,
    hmac_secret: web::Data<HmacSecret>,
) -> Result<HttpResponse, actix_web::Error> {
    // expires in 60 seconds, once the WebSocket is established, the token
    // is no longer needed
    let exp = chrono::Utc::now()
        .checked_add_signed(chrono::Duration::seconds(60))
        .ok_or_else(|| e500(anyhow::anyhow!("time overflow")))?
        .timestamp();

    let claims = ChatClaims {
        sub: user_id.to_string(),
        exp,
        iss: "portfolio-server".to_string(),
    };

    let key = EncodingKey::from_secret(hmac_secret.0.expose_secret().as_bytes());
    let token = encode(&Header::default(), &claims, &key).map_err(e500)?;

    Ok(HttpResponse::Ok().json(serde_json::json!({ "token": token })))
}
