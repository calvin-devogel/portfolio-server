use uuid::Uuid;

use crate::helpers::spawn_app;

#[tokio::test]
async fn users_can_be_created_by_admin() {
    let app = spawn_app().await;
    app.test_user.login(&app).await;

    let new_user = serde_json::json!({
        "username": "new-username",
        "password": "new-password",
    });

    let response = app.post_create_user(&new_user).await;

    assert_eq!(response.status().as_u16(), 202);
}

#[tokio::test]
async fn passwords_can_be_changed() {
    let app = spawn_app().await;
    app.test_user.login(&app).await;
    let new_password = Uuid::new_v4().to_string();

    let old_credentials = serde_json::json!({
        "username": app.test_user.username,
        "password": app.test_user.password,
    });

    let credentials = serde_json::json!({
        "username": app.test_user.username,
        "password": app.test_user.password,
        "new_password": new_password,
    });

    let new_credentials = serde_json::json!({
        "username": app.test_user.username,
        "password": new_password,
    });

    let response = app.post_change_password(&credentials).await;
    assert_eq!(response.status().as_u16(), 202);
    app.post_logout().await;


    let response = app.post_login(&old_credentials).await;
    assert_eq!(response.status().as_u16(), 401);

    let response = app.post_login(&new_credentials).await;
    assert_eq!(response.status().as_u16(), 200);
}

#[tokio::test]
async fn usernames_can_be_queried() {
    let app = spawn_app().await;
    app.test_user.login(&app).await;

    let response = app.get_user_names(None).await;
    assert_eq!(response.status().as_u16(), 200);
    assert!(response.text().await.unwrap().contains("test-user"));
}

#[tokio::test]
async fn usernames_cannot_be_queried_by_anonymous_users() {
    let app = spawn_app().await;

    let response = app.get_user_names(None).await;
    assert_eq!(response.status().as_u16(), 401);
}

#[tokio::test]
async fn users_can_be_queried_by_username() {
    let app = spawn_app().await;
    app.test_user.login(&app).await;

    let response = app.get_user_names(Some("ursula_user".to_string())).await;
    assert_eq!(response.status().as_u16(), 200);
    assert!(response.text().await.unwrap().contains("ursula_user"));
}

#[tokio::test]
async fn admin_can_create_new_users() {
    todo!();
}

#[tokio::test]
async fn anonymous_users_cannot_create_users() {
    todo!();
}

#[tokio::test]
async fn admin_can_change_others_passwords() {
    todo!();
}

#[tokio::test]
async fn anonymous_users_cannot_change_others_passwords() {
    todo!();
}