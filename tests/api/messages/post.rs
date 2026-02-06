use crate::helpers::spawn_app;

#[tokio::test]
async fn can_post_messages() {
    // arrange
    let app = spawn_app().await;

    let message = serde_json::json!({
        "email": "fake@email.com",
        "sender_name": "John Doe",
        "message_text": "Message text.",
    });

    // act
    let response = app.post_message(&message).await;

    // assert
    assert_eq!(response.status().as_u16(), 202);
}

#[tokio::test]
async fn duplicate_messages_are_not_accepted() {
    let app = spawn_app().await;

    let message = serde_json::json!({
        "email": "fake@email.com",
        "sender_name": "John Doe",
        "message_text": "Message text.",
    });

    app.post_message(&message).await;
    let response = app.post_message(&message).await;

    assert_eq!(response.status().as_u16(), 409)
}

#[tokio::test]
async fn invalid_message_emails_are_rejected() {
    let app = spawn_app().await;
    let message = serde_json::json!({
        "email": "fake",
        "name": "John Doe",
        "message_text": "Message text."
    });

    let response = app.post_message(&message).await;

    assert_eq!(response.status().as_u16(), 400)
}