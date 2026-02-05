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