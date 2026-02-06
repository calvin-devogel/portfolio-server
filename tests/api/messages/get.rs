use crate::helpers::spawn_app;

#[tokio::test]
async fn authorized_user_can_query_messages() {
    // arrange
    let app = spawn_app().await;
    app.test_user.login(&app).await;

    // act
    let response = app.get_messages().await;

    // assert
    assert_eq!(response.status().as_u16(), 200);
}

#[tokio::test]
async fn unauthorized_users_cannot_query_messages() {
    let app = spawn_app().await;

    let response = app.get_messages().await;

    assert_eq!(response.status().as_u16(), 401);
}