use crate::helpers::spawn_app;

#[tokio::test]
async fn unauthorized_users_are_unauthorized() {
    // arrange
    let app = spawn_app().await;

    let credentials = serde_json::json!({
        "username": "fake-username",
        "password": "fake-password",
    });

    // act: attempt to log in with fake credentials
    app.post_login(&credentials).await;
    let response = app.check_auth().await;

    // assert
    assert_eq!(response.status().as_u16(), 401);
}

#[tokio::test]
async fn authorized_users_are_authorized() {
    // arrange
    let app = spawn_app().await;
    app.test_user.login(&app).await;

    // act
    let response = app.check_auth().await;

    // assert
    assert_eq!(response.status().as_u16(), 200);
}
