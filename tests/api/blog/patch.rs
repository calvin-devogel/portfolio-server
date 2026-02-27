use crate::helpers::{BlogPostRecord, EditRequest, GetResponse, PublishRequest, spawn_app};

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

    let response = app.publish_blog(&publish_body).await;

    assert_eq!(response.status().as_u16(), 202);

    let response_body = app.get_blog("false", None).await;

    let blogs_response: GetResponse = response_body.json().await.expect("Failed to parse blogs");

    let blog_is_published = blogs_response.data[0].clone();

    assert_eq!(blog_is_published.post_id, publish_body.post_id);
    assert_eq!(blog_is_published.published, true);
}

#[tokio::test]
async fn can_edit_blog_posts() {
    let app = spawn_app().await;
    app.test_user.login(&app).await;

    let blog_body = serde_json::json!({
        "title": "Title",
        "content": "fake post content",
        "excerpt": "fake post...",
        "author": "Andy Admin",
    });

    app.post_blog(&blog_body).await;
    let get_response: GetResponse = app
        .get_blog("false", None)
        .await
        .json()
        .await
        .expect("Failed to get blog json");

    let blog_post = &get_response.data[0];

    let blog_body = BlogPostRecord {
        post_id: blog_post.post_id,
        title: blog_post.title.clone(),
        slug: blog_post.slug.clone(),
        content: blog_post.content.clone(),
        excerpt: blog_post.excerpt.clone(),
        author: blog_post.author.clone(),
        published: blog_post.published,
        created_at: blog_post.created_at,
        updated_at: blog_post.updated_at,
    };

    assert!(blog_body.content.contains("fake post content"));

    let edited_content = EditRequest {
        post_id: blog_post.post_id,
        title: None,
        content: Some("New post content".to_string()),
        excerpt: None,
        author: None
    };

    let response = app.edit_blog(&edited_content).await;
    assert_eq!(response.status().as_u16(), 202);

    let get_response: GetResponse = app
        .get_blog("false", None)
        .await
        .json()
        .await
        .expect("Failed to get blog json");

    let blog_post = &get_response.data[0];

    let blog_body = BlogPostRecord {
        post_id: blog_post.post_id,
        title: blog_post.title.clone(),
        slug: blog_post.slug.clone(),
        content: blog_post.content.clone(),
        excerpt: blog_post.excerpt.clone(),
        author: blog_post.author.clone(),
        published: blog_post.published,
        created_at: blog_post.created_at,
        updated_at: blog_post.updated_at,
    };

    assert!(blog_body.content.contains("New post content"))
}