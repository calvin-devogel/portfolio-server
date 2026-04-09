use crate::helpers::spawn_app;
use totp_rs::{Algorithm, Secret, TOTP};

const TOTP_TEST_SECRET: &str = "JBSWY3DPEHPK3PXPJBSWY3DPEHPK3PX";

#[tokio::test]
async fn totp_setup_requires_authentication() {
    let app = spawn_app().await;

    let response = app.get_totp_setup().await;

    assert_eq!(response.status().as_u16(), 401);
}

#[tokio::test]
async fn totp_setup_returns_otpauth_uri() {
    let app = spawn_app().await;
    app.test_user.login(&app).await;

    let response = app.get_totp_setup().await;

    assert_eq!(response.status().as_u16(), 200);
    let body: serde_json::Value = response.json().await.unwrap();
    let uri = body["otpauth_uri"].as_str().expect("otpauth_uri missing");
    assert!(uri.starts_with("otpauth://totp/"));
}

#[tokio::test]
async fn totp_setup_overwrites_previous_unconfirmed_secret() {
    let app = spawn_app().await;
    app.test_user.login(&app).await;

    let first = app.get_totp_setup().await;
    let second = app.get_totp_setup().await;

    assert_eq!(first.status().as_u16(), 200);
    assert_eq!(second.status().as_u16(), 200);

    let first_uri = first.json::<serde_json::Value>().await.unwrap()["otpauth_uri"]
        .as_str()
        .unwrap()
        .to_string();

    let second_uri = second.json::<serde_json::Value>().await.unwrap()["otpauth_uri"]
        .as_str()
        .unwrap()
        .to_string();

    assert_ne!(first_uri, second_uri);
}

#[tokio::test]
async fn totp_confirm_requires_authentication() {
    let app = spawn_app().await;

    let response = app.post_totp_confirm("123456").await;

    assert_eq!(response.status().as_u16(), 401);
}

#[tokio::test]
async fn totp_confirm_without_setup_returns_400() {
    let app = spawn_app().await;
    app.test_user.login(&app).await;

    let response = app.post_totp_confirm("123456").await;

    assert_eq!(response.status().as_u16(), 400);
}

#[tokio::test]
async fn totp_confirm_with_invalid_code_returns_401() {
    let app = spawn_app().await;
    app.test_user.login(&app).await;
    app.get_totp_setup().await;

    let response = app.post_totp_confirm("000000").await;

    assert_eq!(response.status().as_u16(), 401);
}

#[tokio::test]
async fn totp_confirm_with_valid_code_enables_totp() {
    let app = spawn_app().await;
    app.test_user.login(&app).await;

    let setup_body: serde_json::Value = app.get_totp_setup().await.json().await.unwrap();
    let uri = setup_body["otpauth_uri"].as_str().unwrap();
    let totp = TOTP::from_url(uri).expect("Failed to parse otpauth URI");

    let code = totp.generate_current().unwrap();
    let response = app.post_totp_confirm(&code).await;

    assert_eq!(response.status().as_u16(), 200);

    //verify db flag is set
    let row = sqlx::query!(
        "SELECT totp_enabled FROM users WHERE user_id = $1",
        app.test_user.user_id
    )
    .fetch_one(&app.db_pool)
    .await
    .unwrap();

    assert!(row.totp_enabled);
}

#[tokio::test]
async fn totp_confirm_when_already_enabled_returns_409() {
    let app = spawn_app().await;
    let totp = app.test_user.enable_totp(&app.db_pool).await;
    app.post_login(&app.test_user).await;
    app.post_verify_totp(&totp.generate_current().unwrap())
        .await;

    let response = app.post_totp_confirm("123456").await;

    assert_eq!(response.status().as_u16(), 409);
}

#[tokio::test]
async fn totp_disable_requires_authentication() {
    let app = spawn_app().await;

    let response = app.post_totp_disable(&app.test_user.password.clone()).await;

    assert_eq!(response.status().as_u16(), 401);
}

#[tokio::test]
async fn totp_disable_with_wrong_password_returns_401() {
    let app = spawn_app().await;
    app.test_user.enable_totp(&app.db_pool).await;
    app.test_user.login(&app).await;
    app.post_verify_totp(
        &TOTP::new(
            Algorithm::SHA1,
            6,
            1,
            30,
            Secret::Encoded(TOTP_TEST_SECRET.to_string())
                .to_bytes()
                .unwrap(),
            None,
            "test".to_string(),
        )
        .unwrap()
        .generate_current()
        .unwrap(),
    )
    .await;

    let response = app.post_totp_disable("wrong-password").await;

    assert_eq!(response.status().as_u16(), 401);
}

#[tokio::test]
async fn totp_disable_with_correct_password_clears_totp() {
    let app = spawn_app().await;
    app.test_user.enable_totp(&app.db_pool).await;
    app.test_user.login(&app).await;
    app.post_verify_totp(
        &TOTP::new(
            Algorithm::SHA1,
            6,
            1,
            30,
            Secret::Encoded(TOTP_TEST_SECRET.to_string())
                .to_bytes()
                .unwrap(),
            None,
            "test".to_string(),
        )
        .unwrap()
        .generate_current()
        .unwrap(),
    )
    .await;

    let response = app.post_totp_disable(&app.test_user.password.clone()).await;

    assert_eq!(response.status().as_u16(), 200);

    let row = sqlx::query!(
        "SELECT totp_enabled, totp_secret FROM users WHERE user_id = $1",
        app.test_user.user_id
    )
    .fetch_one(&app.db_pool)
    .await
    .unwrap();

    assert!(!row.totp_enabled);
    assert!(row.totp_secret.is_none());
}

#[tokio::test]
async fn after_disabling_totp_login_returns_200_not_202() {
    let app = spawn_app().await;
    app.test_user.enable_totp(&app.db_pool).await;
    app.test_user.login(&app).await;
    app.post_verify_totp(
        &TOTP::new(
            Algorithm::SHA1,
            6,
            1,
            30,
            Secret::Encoded(TOTP_TEST_SECRET.to_string())
                .to_bytes()
                .unwrap(),
            None,
            "test".to_string(),
        )
        .unwrap()
        .generate_current()
        .unwrap(),
    )
    .await;
    app.post_totp_disable(&app.test_user.password.clone()).await;
    app.post_logout().await;

    let response = app.post_login(&app.test_user).await;

    assert_eq!(response.status().as_u16(), 200);
}
