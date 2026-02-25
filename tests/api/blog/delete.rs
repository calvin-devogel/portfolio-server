use uuid::Uuid;

use crate::helpers::spawn_app;

#[derive(serde::Deserialize, Debug)]
struct BlogsResponse {
    data: Vec<BlogPostRecord>,
}

#[derive(serde::Deserialize, Clone, Debug)]
struct BlogPostRecord {
    post_id: Uuid,
}

#[tokio::test]
async fn authorized_user_can_delete_blogs() {
    let app = spawn_app().await;
    app.test_user.login(&app).await;

    let blog_issue = serde_json::json!({
        "title": "Title",
        "content": "fake blog content",
        "excerpt": "fake blog...",
        "author": "Andy Admin"
    });

    let response = app.post_blog(&blog_issue).await;
    assert_eq!(response.status().as_u16(), 202);

    let response = app.get_blog().await;

    assert_eq!(response.status().as_u16(), 200);
    let blogs_response: BlogsResponse = response
        .json()
        .await
        .expect("Failed to parse blogs");
    
    let blog_post_id = blogs_response.data[0].post_id;

    let blog_to_delete = serde_json::json!({
        "blog_post_id": blog_post_id,
    });

    let response = app.delete_blog(&blog_to_delete).await;
    // let response_body = response.text().await.unwrap();

    // dbg!(response_body);
    // assert!(1 == 2);
    
    assert_eq!(response.status().as_u16(), 200);

    let response = app.get_blog().await;
    let blogs_response: BlogsResponse = response
        .json()
        .await
        .expect("Failed to parse blogs");

    assert!(blogs_response.data.len() == 0);
}