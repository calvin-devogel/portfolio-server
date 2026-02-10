use crate::helpers::spawn_app;

#[derive(serde::Deserialize, Debug)]
struct MessageResponse {
    message: Option<String>,
    message_id: Option<String>,
}

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

    let message_body: MessageResponse = response
        .json()
        .await
        .expect("Failed to deserialize error response.");

    assert!(
        message_body
            .message
            .expect("Something went wrong")
            .contains("Message received successfully")
    );
    assert!(message_body.message_id.is_some());
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

    assert_eq!(response.status().as_u16(), 409);
}

#[tokio::test]
async fn invalid_emails_are_rejected() {
    let app = spawn_app().await;
    let message = serde_json::json!({
        "email": "fake",
        "sender_name": "John Doe",
        "message_text": "Message text."
    });

    let response = app.post_message(&message).await;

    assert_eq!(response.status().as_u16(), 400);
}

#[tokio::test]
async fn rate_limit_enforced_after_three_messages() {
    let app = spawn_app().await;
    let email = "rate_test@example.com";

    // act 1: send 3 messages (should succeed)
    for i in 0..3 {
        let message = serde_json::json!({
            "email": email,
            "sender_name": "Rate Tester",
            "message_text": format!("Message number: {}", i)
        });

        let response = app.post_message(&message).await;
        assert_eq!(response.status().as_u16(), 202);
    }

    // act 2: send a fourth message
    let message = serde_json::json!({
        "email": email,
        "sender_name": "Rate Tester",
        "message_text": "Fourth message should fail",
    });
    let response = app.post_message(&message).await;
    assert_eq!(response.status().as_u16(), 429);
}

#[tokio::test]
async fn sql_injection_attempt_handled_safely() {
    let app = spawn_app().await;
    let message = serde_json::json!({
        "email": "valid@email.com",
        "sender_name": "Robert'; DROP TABLE messages; --",
        "message_text": "'; DELETE FROM messages WHERE '1'='1'",
    });

    let response = app.post_message(&message).await;
    // accept or reject gracefully
    dbg!(response.status().as_u16());
    assert!(response.status().as_u16() == 202 || response.status().as_u16() == 400);
}
