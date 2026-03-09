use crate::helpers::spawn_app;

#[tokio::test]
async fn login_without_totp_returns_200() {
    let app = spawn_app().await;

    let response = app.post_login(&app.test_user).await;

    assert_eq!(response.status().as_u16(), 200);
}

#[tokio::test]
async fn login_with_totp_enabled_returns_202_with_mfa_required() {
    let app = spawn_app().await;

    app.test_user.enable_totp(&app.db_pool).await;

    let response = app.post_login(&app.test_user).await;

    assert_eq!(response.status().as_u16(), 202);
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["mfa_required"], true);
}

#[tokio::test]
async fn mfa_pending_session_cannot_access_admin_routes() {
    let app = spawn_app().await;
    app.test_user.enable_totp(&app.db_pool).await;

    app.post_login(&app.test_user).await;

    let response = app.get_messages().await;
    assert_eq!(response.status().as_u16(), 401);
}

#[tokio::test]
async fn verify_totp_without_pending_session_returns_401() {
    let app = spawn_app().await;

    let response = app.post_verify_totp("123456").await;

    assert_eq!(response.status().as_u16(), 401);
}

#[tokio::test]
async fn verify_totp_with_valid_code_returns_200() {
    let app = spawn_app().await;
    let totp = app.test_user.enable_totp(&app.db_pool).await;
    app.post_login(&app.test_user).await;

    let code = totp.generate_current().unwrap();
    let response = app.post_verify_totp(&code).await;

    assert_eq!(response.status().as_u16(), 200);
}

#[tokio::test]
async fn verify_totp_promotes_session_and_grants_admin_access() {
    let app = spawn_app().await;
    let totp = app.test_user.enable_totp(&app.db_pool).await;
    app.post_login(&app.test_user).await;

    let code = totp.generate_current().unwrap();
    app.post_verify_totp(&code).await;

    let response = app.get_messages().await;
    assert_eq!(response.status().as_u16(), 200);
}

#[tokio::test]
async fn verify_totp_with_invalid_code_returns_401() {
    let app = spawn_app().await;
    app.test_user.enable_totp(&app.db_pool).await;

    let response = app.post_verify_totp("000000").await;

    assert_eq!(response.status().as_u16(), 401);
}

#[tokio::test]
async fn invalid_totp_code_does_not_invalidate_pending_session() {
    let app = spawn_app().await;
    let totp = app.test_user.enable_totp(&app.db_pool).await;
    app.post_login(&app.test_user).await;

    // wrong code, session is pending
    let bad_response = app.post_verify_totp("not-a-code").await;
    assert_eq!(bad_response.status().as_u16(), 401);

    // correct code: should work
    let code = totp.generate_current().unwrap();
    let good_response = app.post_verify_totp(&code).await;
    assert_eq!(good_response.status().as_u16(), 200);
}
