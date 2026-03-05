use crate::helpers::{GetResponse, PublishRequest, spawn_app};

// how to destructure pagination query:
// page i64
// page_size i64 that's it
// but how to read the response?
#[tokio::test]
async fn can_query_articles() {
    let app = spawn_app().await;

    let response = app.get_article("true", None).await;

    assert_eq!(response.status().as_u16(), 200);
}

#[tokio::test]
async fn articles_are_returned_when_they_exist() {
    let app = spawn_app().await;
    app.test_user.login(&app).await;

    let article = serde_json::json!({
        "title": "Title",
        "sections": [{"type": "markdown", "content": "fake post content..."}],
        "excerpt": "fake post...",
        "author": "Andy Admin"
    });

    let post_response = app.post_article(&article).await;
    assert_eq!(post_response.status().as_u16(), 202);

    let response = app.get_article("false", None).await;
    let response_body = response.text().await.unwrap();
    assert!(response_body.contains("fake post content"));
}

#[tokio::test]
async fn articles_can_be_filtered_on_published() {
    let app = spawn_app().await;
    app.test_user.login(&app).await;

    let article_published = serde_json::json!({
        "title": "Title",
        "sections": [{"type": "markdown", "content": "fake post content..."}],
        "excerpt": "fake post...",
        "author": "Andy Admin"
    });

    let article_unpublished = serde_json::json!({
        "title": "Do Not Publish",
        "sections": [{"type": "markdown", "content": "unpublished fake post..."}],
        "excerpt": "unpublished...",
        "author": "Andy Admin"
    });

    let post_response = app.post_article(&article_published).await;
    assert_eq!(post_response.status().as_u16(), 202);
    let post_response = app.post_article(&article_unpublished).await;
    assert_eq!(post_response.status().as_u16(), 202);

    let article_response: GetResponse = app
        .get_article("false", Some("title".to_string()))
        .await
        .json()
        .await
        .expect("Failed to get blog json");

    let publish_body = PublishRequest {
        post_id: article_response.data[0].post_id,
        published: true,
    };

    dbg!(&publish_body.post_id);

    let response = app.publish_article(&publish_body).await;
    dbg!(&response);
    assert_eq!(response.status().as_u16(), 202);

    let response = app.get_article("true", None).await;
    let get_response: GetResponse = response.json().await.expect("Failed to get response json");

    assert_eq!(get_response.data.len(), 1);
}

#[tokio::test]
async fn blogs_can_be_filtered_on_slug() {
    let app = spawn_app().await;
    app.test_user.login(&app).await;

    let article_published = serde_json::json!({
        "title": "Title",
        "sections": [{"type": "markdown", "content": "fake post content..."}],
        "excerpt": "fake post...",
        "author": "Andy Admin"
    });

    let article_unpublished = serde_json::json!({
        "title": "Do Not Publish",
        "sections": [{"type": "markdown", "content": "unpublished fake post..."}],
        "excerpt": "unpublished...",
        "author": "Andy Admin"
    });

    let post_response = app.post_article(&article_published).await;
    assert_eq!(post_response.status().as_u16(), 202);
    let post_response = app.post_article(&article_unpublished).await;
    assert_eq!(post_response.status().as_u16(), 202);

    let response = app
        .get_article("false", Some("do-not-publish".to_string()))
        .await;
    let get_response: GetResponse = response.json().await.expect("Failed to get response json");

    assert_eq!(get_response.data.len(), 1);
}
