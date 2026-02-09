use crate::helpers::spawn_app;

#[derive(serde::Deserialize, Debug)]
struct _MessageRecord {
    message_id: uuid::Uuid,
    email: String,
    sender_name: String,
    message_text: String,
    created_at: chrono::DateTime<chrono::Utc>,
    read_message: Option<bool>,
}

#[derive(serde::Deserialize, Debug)]
struct _MessagesResponse {
    messages: Vec<_MessageRecord>,
    page: i64,
    page_size: i64,
    total_count: i64,
}

#[tokio::test]
async fn authorized_user_can_query_messages() {
    // arrange
    let app = spawn_app().await;
    app.test_user.login(&app).await;

    // act
    let response = app.get_messages().await;

    // assert
    assert_eq!(response.status().as_u16(), 200);
}

#[tokio::test]
async fn unauthorized_users_cannot_query_messages() {
    let app = spawn_app().await;

    let response = app.get_messages().await;

    assert_eq!(response.status().as_u16(), 401);
}

#[tokio::test]
async fn messages_are_returned_when_they_exist() {
    let app = spawn_app().await;
    app.test_user.login(&app).await;

    let message = serde_json::json!({
        "email": "valid@email.com",
        "sender_name": "John Doe",
        "message_text": "This is a test message"
    });

    app.post_message(&message).await;
    let response = app.get_messages().await;

    let response_body = response.text().await.unwrap();
    assert!(response_body.contains("This is a test message"));
}
