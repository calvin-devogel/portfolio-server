use uuid::Uuid;

use crate::helpers::spawn_app;

#[derive(serde::Deserialize, Debug)]
struct ArticleResponse {
    data: Vec<ArticleRecord>,
}

#[derive(serde::Deserialize, Clone, Debug)]
struct ArticleRecord {
    post_id: Uuid,
}

#[tokio::test]
async fn authorized_user_can_delete_articles() {
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

    let response = app.get_article("false", None).await;

    assert_eq!(response.status().as_u16(), 200);
    let blogs_response: ArticleResponse = response.json().await.expect("Failed to parse blogs");

    let blog_post_id = blogs_response.data[0].post_id;

    let blog_to_delete = serde_json::json!({
        "post_id": blog_post_id,
    });

    let response = app.delete_article(&blog_to_delete).await;
    assert_eq!(response.status().as_u16(), 200);

    let response = app.get_article("false", None).await;
    let blogs_response: ArticleResponse = response.json().await.expect("Failed to parse blogs");

    assert!(blogs_response.data.len() == 0);
}

#[tokio::test]
async fn deleting_nonexistent_article_returns_not_found() {
    let app = spawn_app().await;
    app.test_user.login(&app).await;

    let body = serde_json::json!({ "post_id": Uuid::new_v4() });
    let response = app.delete_article(&body).await;
    assert_eq!(response.status().as_u16(), 404);
}