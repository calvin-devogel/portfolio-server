use uuid::Uuid;

use crate::helpers::spawn_app;

#[derive(serde::Serialize)]
struct MessageToPatch {
    message_id: Uuid,
    read: bool,
}

#[derive(serde::Deserialize, Debug, Clone)]
struct MessageRecord {
    message_id: Uuid,
    read_message: Option<bool>,
}

#[derive(serde::Deserialize, Debug)]
struct MessagesResponse {
    messages: Vec<MessageRecord>,
}

#[tokio::test]
async fn authorized_user_can_patch_messages() {
    let app = spawn_app().await;
    app.test_user.login(&app).await;

    // act 1: send a message
    let message = serde_json::json!({
        "email": "fake@email.com",
        "sender_name": "John Doe",
        "message_text": "Message text.",
    });
    app.post_message(&message).await;

    // act 2: get posted messages
    let response_body = app.get_messages().await;

    let messages_response: MessagesResponse = response_body
        .json()
        .await
        .expect("Failed to parse messages response");

    let message_id = messages_response.messages[0].message_id;

    let patch_body = MessageToPatch {
        message_id,
        read: true,
    };

    // act 3: attempt to patch
    let response = app.patch_message(&patch_body).await;
    assert_eq!(response.status().as_u16(), 202);

    // act 4: get patched message
    let response_body = app.get_messages().await;

    let messages_response: MessagesResponse = response_body
        .json()
        .await
        .expect("Failed to parse messages");

    let message_is_read = messages_response.messages[0].clone();

    assert_eq!(message_is_read.message_id, message_id);
    assert_eq!(message_is_read.read_message, Some(true));
}

#[tokio::test]
async fn unauthorized_users_cannot_patch_messages() {
    let app = spawn_app().await;
    app.test_user.login(&app).await;

    // act 1: send a message
    let message = serde_json::json!({
        "email": "fake@email.com",
        "sender_name": "John Doe",
        "message_text": "Message text."
    });
    app.post_message(&message).await;

    // act 2: get the posted messages
    let response_body = app.get_messages().await;

    let messages_response: MessagesResponse = response_body
        .json()
        .await
        .expect("Failed to parse messages response");

    let message_id = messages_response.messages[0].message_id;

    let patch_body = MessageToPatch {
        message_id,
        read: true,
    };

    // act 3: logout
    app.post_logout().await;

    // act 4: attempt to patch
    let response = app.patch_message(&patch_body).await;
    assert_eq!(response.status().as_u16(), 401);
}

// test what happen when when the message is not found
// and for idempotent tries re-using the same key
#[tokio::test]
async fn try_to_patch_non_existent_message() {
    let app = spawn_app().await;
    app.test_user.login(&app).await;

    let fake_message = MessageToPatch {
        message_id: Uuid::new_v4(),
        read: true
    };

    let response = app.patch_message(&fake_message).await;
    assert_eq!(response.status().as_u16(), 404);
}

#[tokio::test]
async fn try_to_patch_with_reused_idempotency_key() {
    let app = spawn_app().await;
    app.test_user.login(&app).await;

    let message = serde_json::json!({
        "email": "fake@email.com",
        "sender_name": "John Doe",
        "message_text": "Message text."
    });
    app.post_message(&message).await;

    let response_body = app.get_messages().await;

    let messages_response: MessagesResponse = response_body
        .json()
        .await
        .expect("Failed to parse messages response");

    let message_id = messages_response.messages[0].message_id;

    let patch_body = MessageToPatch {
        message_id,
        read: true,
    };

    let idempotency_key = Uuid::new_v4();

    let first_response = app.patch_message_with_reused_key(&patch_body, &idempotency_key).await;
    let second_response = app.patch_message_with_reused_key(&patch_body, &idempotency_key).await;

    assert_eq!(first_response.status().as_u16(), second_response.status().as_u16());
}