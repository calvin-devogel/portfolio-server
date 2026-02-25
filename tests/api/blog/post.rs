use crate::helpers::spawn_app;


#[tokio::test]
async fn unauthorized_users_cannot_post_blogs() {
    let app = spawn_app().await;

    let blog_body = serde_json::json!({
        "title": "Title",
        "content": "fake post content",
        "excerpt": "fake post...",
        "author": "Henry Hacker"
    });

    let response = app.post_blog(&blog_body).await;
    assert_eq!(response.status().as_u16(), 401);
}

#[tokio::test]
async fn authorized_users_can_post_blogs() {
    let app = spawn_app().await;
    app.test_user.login(&app).await;

    let blog_body = serde_json::json!({
        "title": "Title",
        "content": "fake post content",
        "excerpt": "fake post...",
        "author": "Andy Admin"
    });

    let response = app.post_blog(&blog_body).await;
    assert_eq!(response.status().as_u16(), 202);
}

#[tokio::test]
async fn blog_posts_with_bad_data_are_rejected() {
    let app = spawn_app().await;
    app.test_user.login(&app).await;

    let blog_body = serde_json::json!({
        "title": "Title",
        "conent": "fake post content",
        "excerpt": "fake post...",
    });

    let response = app.post_blog(&blog_body).await;
    dbg!(&response.status().as_u16());
    assert_eq!(response.status().as_u16(), 400)
}