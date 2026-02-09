use crate::helpers::spawn_app;

#[tokio::test]
async fn health_check_reports_correctly() {
    // arrange
    let app = spawn_app().await;

    // act
    let response = app.generic_request().await;

    // assert
    assert_eq!(response.status().as_u16(), 200);
}
