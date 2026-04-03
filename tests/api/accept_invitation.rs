use chrono::Utc;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::helpers::spawn_app;

async fn create_token(app: &crate::helpers::TestApp) -> String {
    app.test_user.login(app).await;

    let response = app
        .post_create_user(&serde_json::json!({
            "email": format!("{}@example.com", Uuid::new_v4()),
            "role": "user",
        }))
        .await;

    let body: serde_json::Value = response.json().await.expect("Response should be JSON");
    let link = body["link"].as_str().expect("Link should be present");
    let token = link
        .split("token=")
        .last()
        .expect("Token should be in link")
        .to_string();

    app.post_logout().await;
    token
}

#[tokio::test]
async fn valid_invitation_token_can_be_accepted() {
    let app = spawn_app().await;
    let token = create_token(&app).await;

    let response = app
        .post_accept_invitation(&serde_json::json!({
            "token": token,
            "username": Uuid::new_v4().to_string(),
            "password": "SecurePassword123!",
        }))
        .await;

    assert_eq!(response.status().as_u16(), 200);
}

#[tokio::test]
async fn accepted_user_can_login() {
    let app = spawn_app().await;
    let token = create_token(&app).await;

    let username = Uuid::new_v4().to_string();
    let password = "SecurePassword123!";

    app.post_accept_invitation(&serde_json::json!({
        "token": token,
        "username": &username,
        "password": &password,
    }))
    .await;

    let login_response = app
        .post_login(&serde_json::json!({
            "username": &username,
            "password": &password,
        }))
        .await;

    assert_eq!(login_response.status().as_u16(), 200);
}

#[tokio::test]
async fn invitation_token_cannot_be_reused() {
    let app = spawn_app().await;
    let token = create_token(&app).await;

    app.post_accept_invitation(&serde_json::json!({
        "token": &token,
        "username": Uuid::new_v4().to_string(),
        "password": "SecurePassword123!",
    }))
    .await;

    let second_response = app
        .post_accept_invitation(&serde_json::json!({
            "token": token,
            "username": Uuid::new_v4().to_string(),
            "password": "SecurePassword123!",
        }))
        .await;

    assert_eq!(second_response.status().as_u16(), 400);
}

#[tokio::test]
async fn invalid_token_is_rejected() {
    let app = spawn_app().await;

    let response = app
        .post_accept_invitation(&serde_json::json!({
            "token": "invalid-token",
            "username": Uuid::new_v4().to_string(),
            "password": "SecurePassword123!",
        }))
        .await;

    assert_eq!(response.status().as_u16(), 400);
}

#[tokio::test]
async fn expired_token_is_rejected() {
    let app = spawn_app().await;

    let raw_token = Uuid::new_v4().to_string();
    let mut hasher = Sha256::new();
    hasher.update(raw_token.as_bytes());
    let token_hash = hex::encode(hasher.finalize());
    let expired_at = Utc::now() - chrono::Duration::hours(25);

    sqlx::query!(
        r#"
        INSERT INTO user_invitations (id, email, role, invitation_token_hash, expires_at, created_at)
        VALUES ($1, $2, $3, $4, $5, NOW())
        "#,
        Uuid::new_v4(),
        "expired@example.com",
        "user",
        token_hash,
        expired_at
    )
    .execute(&app.db_pool)
    .await
    .expect("Failed to insert expired token");

    let response = app
        .post_accept_invitation(&serde_json::json!({
            "token": raw_token,
            "username": Uuid::new_v4().to_string(),
            "password": "SecurePassword123!",
        }))
        .await;

    assert_eq!(response.status().as_u16(), 400);
}
