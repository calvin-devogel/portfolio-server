use crate::helpers::{ArticleRecord, ArticleSection, EditRequest, GetResponse, PublishRequest, spawn_app};

#[tokio::test]
async fn authorized_user_can_publish_articles() {
    let app = spawn_app().await;
    app.test_user.login(&app).await;

    let article = serde_json::json!({
        "title": "Title",
        "sections": [{"type": "markdown", "content": "fake post content..."}],
        "excerpt": "fake blog...",
        "author": "Andy Admin"
    });

    let post_response = app.post_article(&article).await;

    assert_eq!(post_response.status().as_u16(), 202);

    let response = app.get_article("false", None).await;

    dbg!(&response.status());
    // let response_text =

    // dbg!(response_text);
    let article_response: GetResponse = response.json().await.expect("Failed to parse blogs");

    let publish_body = PublishRequest {
        post_id: article_response.data[0].post_id,
        published: true,
    };

    let response = app.publish_article(&publish_body).await;

    assert_eq!(response.status().as_u16(), 202);

    let response_body = app.get_article("false", None).await;

    let blogs_response: GetResponse = response_body.json().await.expect("Failed to parse blogs");

    let blog_is_published = blogs_response.data[0].clone();

    assert_eq!(blog_is_published.post_id, publish_body.post_id);
    assert_eq!(blog_is_published.published, true);
}

#[tokio::test]
async fn can_edit_articles() {
    let app = spawn_app().await;
    app.test_user.login(&app).await;

    let article = serde_json::json!({
        "title": "Title",
        "sections": [{"type": "markdown", "content": "fake post content..."}],
        "excerpt": "fake blog...",
        "author": "Andy Admin"
    });

    app.post_article(&article).await;
    let get_response: GetResponse = app
        .get_article("false", None)
        .await
        .json()
        .await
        .expect("Failed to get blog json");

    let article_record = &get_response.data[0];

    let article_body = ArticleRecord {
        post_id: article_record.post_id,
        title: article_record.title.clone(),
        slug: article_record.slug.clone(),
        sections: article_record.sections.clone(),
        excerpt: article_record.excerpt.clone(),
        author: article_record.author.clone(),
        published: article_record.published,
        created_at: article_record.created_at,
        updated_at: article_record.updated_at,
    };

    let article_section = article_body.sections.first().unwrap().clone();

    let content = match article_section {
        ArticleSection::Markdown { content } => content.clone(),
        ArticleSection::Carousel { label, .. } => label.clone(),
    };

    assert!(content.contains("fake post content..."));

    let edited_content = EditRequest {
        post_id: article_body.post_id,
        title: None,
        // content: Some("New post content".to_string()),
        sections: Some(vec![serde_json::json!({
            "type": "markdown",
            "content": "edited post content"
        })]),
        excerpt: None,
        author: None
    };

    let response = app.edit_article(&edited_content).await;
    assert_eq!(response.status().as_u16(), 202);

    let get_response: GetResponse = app
        .get_article("false", None)
        .await
        .json()
        .await
        .expect("Failed to get blog json");

    let article_record = &get_response.data[0];

    let article_body = ArticleRecord {
        post_id: article_record.post_id,
        title: article_record.title.clone(),
        slug: article_record.slug.clone(),
        sections: article_record.sections.clone(),
        excerpt: article_record.excerpt.clone(),
        author: article_record.author.clone(),
        published: article_record.published,
        created_at: article_record.created_at,
        updated_at: article_record.updated_at,
    };

    let article_section = article_body.sections.first().unwrap().clone();

    let content = match article_section {
        ArticleSection::Markdown { content } => content.clone(),
        ArticleSection::Carousel { label, .. } => label.clone(),
    };

    assert!(content.contains("edited post content"));
}