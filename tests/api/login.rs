use crate::helpers::spawn_app;

#[tokio::test]
async fn unauthorized_users_are_rejected() {
    // arrange
    let app = spawn_app().await;

    // act
    let login_body = serde_json::json!({
        "username": "random-username",
        "password": "random-password",
    });
    let response = app.post_login(&login_body).await;

    // assert
    assert_eq!(response.status().as_u16(), 401);
}

#[tokio::test]
async fn authorized_users_can_login() {
    // arrange
    let app = spawn_app().await;

    // act
    let response = app.post_login(&app.test_user).await;

    // assert
    assert_eq!(response.status().as_u16(), 200);
}

#[tokio::test]
async fn unauthorized_users_cannot_access_restricted_routes() {
    //arrange
    let app = spawn_app().await;

    // act
    let response = app.test_reject().await;

    // assert
    assert_eq!(response.status().as_u16(), 401);
}

#[tokio::test]
async fn authorized_users_can_access_restricted_routes() {
    // arrange
    let app = spawn_app().await;
    app.test_user.login(&app).await;

    // act
    let response = app.test_reject().await;

    // assert
    assert_eq!(response.status().as_u16(), 200);
}