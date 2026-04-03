use jsonwebtoken::dangerous::insecure_decode;

use crate::helpers::spawn_app;

#[derive(serde::Deserialize)]
struct ChatTokenResponse {
    token: String,
}

#[derive(serde::Deserialize)]
struct TestClaims {
    name: String,
    sub: String,
    exp: i64,
    iss: String,
}

#[tokio::test]
async fn unauthenticated_user_cannot_get_chat_token() {
    let app = spawn_app().await;

    let response = app.get_chat_token().await;

    assert_eq!(response.status().as_u16(), 401);
}

#[tokio::test]
async fn chat_token_is_not_accessible_after_logout() {
    let app = spawn_app().await;
    app.test_user.login(&app).await;
    app.post_logout().await;

    let response = app.get_chat_token().await;

    assert_eq!(response.status().as_u16(), 401);
}

#[tokio::test]
async fn authenticated_user_receives_chat_token() {
    let app = spawn_app().await;
    app.test_user.login(&app).await;

    let response = app.get_chat_token().await;

    assert_eq!(response.status().as_u16(), 200);
    let body: ChatTokenResponse = response
        .json()
        .await
        .expect("Response was not a JSON object with a `token` field");
    assert!(!body.token.is_empty());
}

#[tokio::test]
async fn chat_token_uses_es256_algorithm() {
    let app = spawn_app().await;
    app.test_user.login(&app).await;

    let body: ChatTokenResponse = app.get_chat_token().await.json().await.unwrap();
    let token_data =
        insecure_decode::<TestClaims>(&body.token).expect("Failed to decode JWT claims");

    let claims = token_data.claims;

    assert_eq!(claims.sub, app.test_user.user_id.to_string());
    assert_eq!(claims.name, app.test_user.username);
    assert_eq!(claims.iss, "portfolio-server");
}

#[tokio::test]
async fn chat_token_expires_within_ten_seconds() {
    let app = spawn_app().await;
    app.test_user.login(&app).await;

    let before = chrono::Utc::now().timestamp();
    let body: ChatTokenResponse = app.get_chat_token().await.json().await.unwrap();
    let claims = insecure_decode::<TestClaims>(&body.token).unwrap().claims;

    assert!(claims.exp > before, "token exp is already in the past");

    assert!(
        claims.exp <= before + 15,
        "token exp is too far in the future: {} (expected <= {})",
        claims.exp,
        before + 15
    );
}
