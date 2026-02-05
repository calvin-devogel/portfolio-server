use uuid::Uuid;
use chrono::Utc;
use crate::helpers::spawn_app;

#[tokio::test]
async fn can_post_messages() {
    // arrange
    let app = spawn_app().await;

    let message_text = serde_json::json!({
        "message_id": Uuid::new_v4().to_string(),
        "email": "fake@email.com",
        "sender_name": "John Doe",
        "message_text": "Message text.",
        "created_at": Utc::now().to_string()
    });

    // act
    let response = app.post_message(&message_text).await;

    // assert
    assert_eq!(response.status().as_u16(), 202);
}