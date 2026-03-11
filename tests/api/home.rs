use crate::helpers::spawn_app;

#[tokio::test]
async fn get_home_returns_200_and_html() {
    let app = spawn_app().await;

    let response = app.get_home().await;
    assert_eq!(response.status().as_u16(), 200);

    let content_type = response.headers().get("Content-Type").unwrap();
    assert_eq!(content_type, "text/html; charset=utf-8");
    let body = response.text().await.unwrap();
    assert!(body.contains("<h1>Hey! You shouldn't be here.</h1>"));
}
