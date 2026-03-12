use crate::helpers::spawn_app;

#[tokio::test]
async fn unauthorized_users_cannot_post_articles() {
    let app = spawn_app().await;

    let article = serde_json::json!({
        "title": "Title",
        "sections": [{"type": "markdown", "content": "fake post content..."}],
        "excerpt": "fake blog...",
        "author": "Henry Hacker"
    });

    let response = app.post_article(&article).await;
    assert_eq!(response.status().as_u16(), 401);
}

#[tokio::test]
async fn authorized_users_can_post_articles() {
    let app = spawn_app().await;
    app.test_user.login(&app).await;

    let article = serde_json::json!({
        "title": "Title",
        "sections": [{"type": "markdown", "content": "fake post content..."}],
        "excerpt": "fake blog...",
        "author": "Andy Admin"
    });

    let response = app.post_article(&article).await;
    assert_eq!(response.status().as_u16(), 202);
}

#[tokio::test]
async fn blog_posts_with_bad_data_are_rejected() {
    let app = spawn_app().await;
    app.test_user.login(&app).await;

    let blog_body = serde_json::json!({
        "title": "Title",
        "content": "fake post content",
        "excerpt": "fake post...",
    });

    let response = app.post_article(&blog_body).await;
    dbg!(&response.status().as_u16());
    assert_eq!(response.status().as_u16(), 413);

    let blog_body = serde_json::json!({
        "title": "Title",
        "sections": [],
        "excerpt": "fake post...",
        "author": "Andy Admin"
    });

    let response = app.post_article(&blog_body).await;
    assert_eq!(response.status().as_u16(), 400);
}

#[tokio::test]
async fn posting_duplicate_article_returns_conflict() {
    let app = spawn_app().await;
    app.test_user.login(&app).await;

    let article = serde_json::json!({
        "title": "duplicate title",
        "sections": [{"type": "markdown", "content": "fake post content..."}],
        "excerpt": "fake blog...",
        "author": "Andy Admin"
    });

    let first = app.post_article(&article).await;
    assert_eq!(first.status().as_u16(), 202);

    let second = app.post_article(&article).await;
    assert_eq!(second.status().as_u16(), 409);
}