use crate::helpers::{spawn_app, PublishRequest, GetResponse};

#[tokio::test]
async fn authorized_user_can_publish_blogs() {
    let app = spawn_app().await;
    app.test_user.login(&app).await;

    let blog_body = serde_json::json!({
        "title": "Title",
        "content": "fake post content",
        "excerpt": "fake post...",
        "author": "Andy Admin",
    });

    let post_response = app.post_blog(&blog_body).await;

    assert_eq!(post_response.status().as_u16(), 202);

    let response = app.get_blog("false", None).await;

    dbg!(&response.status());
    // let response_text =

    // dbg!(response_text);
    let blogs_response: GetResponse = response.json().await.expect("Failed to parse blogs");

    let publish_body = PublishRequest {
        post_id: blogs_response.data[0].post_id,
        published: true,
    };

    let response = app.patch_blog(&publish_body).await;

    assert_eq!(response.status().as_u16(), 202);

    let response_body = app.get_blog("false", None).await;

    let blogs_response: GetResponse = response_body.json().await.expect("Failed to parse blogs");

    let blog_is_published = blogs_response.data[0].clone();

    assert_eq!(blog_is_published.post_id, publish_body.post_id);
    assert_eq!(blog_is_published.published, true);
}
