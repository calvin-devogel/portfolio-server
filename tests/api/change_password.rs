use uuid::Uuid;

use crate::helpers::spawn_app;

#[tokio::test]
async fn unauthenticated_users_cannot_change_password() {
    let app = spawn_app().await;

    let body = serde_json::json!({
        "current_password": "some-password",
        "new_password": "some-new-password",
    });

    let response = app.post_change_password(&body).await;
    assert_eq!(response.status().as_u16(), 401);
}

#[tokio::test]
async fn authenticated_users_can_change_password_with_correct_current_password() {
    let app = spawn_app().await;
    app.test_user.login(&app).await;

    let body = serde_json::json!({
        "current_password": &app.test_user.password,
        "new_password": "NewSecurePassword456!",
    });

    let response = app.post_change_password(&body).await;
    assert_eq!(response.status().as_u16(), 202);
}

#[tokio::test]
async fn wrong_current_password_is_rejected() {
    let app = spawn_app().await;
    app.test_user.login(&app).await;

    let body = serde_json::json!({
        "current_password": "definitely-the-wrong-password",
        "new_password": "NewSecurePassword456!",
    });

    let response = app.post_change_password(&body).await;
    assert_eq!(response.status().as_u16(), 401);
}

#[tokio::test]
async fn user_can_login_with_new_password_after_change() {
    let app = spawn_app().await;
    app.test_user.login(&app).await;

    let new_password = Uuid::new_v4().to_string();
    let body = serde_json::json!({
        "current_password": &app.test_user.password,
        "new_password": &new_password,
    });

    let change_response = app.post_change_password(&body).await;
    assert_eq!(change_response.status().as_u16(), 202);

    app.post_logout().await;

    let login_response = app
        .post_login(&serde_json::json!({
            "username": &app.test_user.username,
            "password": &new_password,
        }))
        .await;
    assert_eq!(login_response.status().as_u16(), 200);
}

#[tokio::test]
async fn old_password_no_longer_works_after_change() {
    let app = spawn_app().await;
    app.test_user.login(&app).await;

    let body = serde_json::json!({
        "current_password": &app.test_user.password,
        "new_password": Uuid::new_v4().to_string(),
    });
    app.post_change_password(&body).await;
    app.post_logout().await;

    let login_response = app
        .post_login(&serde_json::json!({
            "username": &app.test_user.username,
            "password": &app.test_user.password,
        }))
        .await;
    assert_eq!(login_response.status().as_u16(), 401);
}

#[tokio::test]
async fn must_change_password_flag_is_cleared_after_password_change() {
    let app = spawn_app().await;
    app.set_must_change_password(app.test_user.user_id).await;
    app.test_user.login(&app).await;

    let body = serde_json::json!({
        "current_password": &app.test_user.password,
        "new_password": Uuid::new_v4().to_string(),
    });
    app.post_change_password(&body).await;

    let flag = app
        .get_must_change_password_flag(app.test_user.user_id)
        .await;
    assert!(
        !flag,
        "must_change_password flag should be cleared after password change"
    );
}

#[tokio::test]
async fn login_signals_must_change_password_in_response_body() {
    let app = spawn_app().await;
    app.set_must_change_password(app.test_user.user_id).await;

    let response = app
        .post_login(&serde_json::json!({
            "username": &app.test_user.username,
            "password": &app.test_user.password,
        }))
        .await;

    assert_eq!(response.status().as_u16(), 200);
    let body: serde_json::Value = response
        .json()
        .await
        .expect("Response should be valid JSON");
    assert_eq!(body["must_change_password"], true);
}

#[tokio::test]
async fn normal_login_does_not_include_must_change_password_flag() {
    let app = spawn_app().await;

    let response = app
        .post_login(&serde_json::json!({
            "username": &app.test_user.username,
            "password": &app.test_user.password,
        }))
        .await;

    assert_eq!(response.status().as_u16(), 200);
    let body = response
        .bytes()
        .await
        .expect("Failed to read response bytes");
    assert!(
        body.is_empty(),
        "Response body should be empty for normal login"
    );
}
