use crate::helpers::spawn_app;

// how to destructure pagination query:
// page i64
// page_size i64 that's it
// but how to read the response?
#[tokio::test]
async fn can_query_blog_posts() {
    let app = spawn_app().await;

    let response = app.get_blog("true").await;

    assert_eq!(response.status().as_u16(), 200);
}

#[tokio::test]
async fn blogs_are_returned_when_they_exist() {
    let app = spawn_app().await;
    app.test_user.login(&app).await;

    let blog_body = serde_json::json!({
        "title": "Title",
        "content": "fake post content",
        "excerpt": "fake post...",
        "author": "Andy Admin"
    });

    let post_response = app.post_blog(&blog_body).await;
    assert_eq!(post_response.status().as_u16(), 202);


    let response = app.get_blog("false").await;
    let response_body = response.text().await.unwrap();
    assert!(response_body.contains("fake post content"));
}
