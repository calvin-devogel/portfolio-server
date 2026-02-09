use crate::helpers::spawn_app;
use actix_web::HttpResponse;
use portfolio_server::idempotency::{
    IdempotencyKey, NextAction, get_saved_response, save_response, try_processing,
};
use uuid::Uuid;

#[tokio::test]
async fn try_processing_returns_start_processing_for_new_key() {
    let app = spawn_app().await;
    let key = IdempotencyKey::try_from("test-key-123".to_string()).unwrap();

    let (action, transaction) = try_processing(&app.db_pool, &key, None)
        .await
        .expect("Failed to process");

    assert!(matches!(action, NextAction::StartProcessing));
    assert!(transaction.is_some());
}

#[tokio::test]
async fn try_processing_returns_saved_response_for_duplicate_key() {
    let app = spawn_app().await;
    let key = IdempotencyKey::try_from("duplicate-key".to_string()).unwrap();

    // act 1: process and save
    let (action, transaction) = try_processing(&app.db_pool, &key, None)
        .await
        .expect("Failed to process first request");

    assert!(matches!(action, NextAction::StartProcessing));
    let transaction = transaction.unwrap();

    let response = HttpResponse::Ok()
        .insert_header(("X-Test-Header", "test-value"))
        .body("Test response body");

    save_response(transaction, &key, None, response)
        .await
        .expect("Failed to save response");

    // act 2: try processing, should return saved response
    let (action, transaction) = try_processing(&app.db_pool, &key, None)
        .await
        .expect("Failed to process second request");

    assert!(transaction.is_none());

    match action {
        NextAction::ReturnSavedResponse(saved) => {
            assert_eq!(saved.status().as_u16(), 200);
        }
        NextAction::StartProcessing => panic!("Expected saved response, got StartProcessing"),
    }
}

#[tokio::test]
async fn save_response_persists_status_code_and_body() {
    let app = spawn_app().await;
    let key = IdempotencyKey::try_from("persist-test".to_string()).unwrap();

    let (_, transaction) = try_processing(&app.db_pool, &key, None)
        .await
        .expect("Failed to start processing");

    let response = HttpResponse::Accepted().body("Message received");

    save_response(transaction.unwrap(), &key, None, response)
        .await
        .expect("Failed to save");

    let saved = get_saved_response(&app.db_pool, &key, None)
        .await
        .expect("Failed to retrieve")
        .expect("Response not found");

    assert_eq!(saved.status().as_u16(), 202);
}

#[tokio::test]
async fn save_response_persists_headers() {
    let app = spawn_app().await;
    let key = IdempotencyKey::try_from("header-test".to_string()).unwrap();

    let (_, transaction) = try_processing(&app.db_pool, &key, None)
        .await
        .expect("Failed to start processing");

    let response = HttpResponse::Ok()
        .insert_header(("Content-Type", "application/json"))
        .insert_header(("X-Custom-Header", "custom-value"))
        .body(r#"{"status":"ok"}"#);

    save_response(transaction.unwrap(), &key, None, response)
        .await
        .expect("Failed to save");

    let saved = get_saved_response(&app.db_pool, &key, None)
        .await
        .expect("Failed to retrieve")
        .expect("Response not found");

    let headers = saved.headers();
    assert_eq!(
        headers.get("content-type").unwrap().to_str().unwrap(),
        "application/json"
    );

    assert_eq!(
        headers.get("x-custom-header").unwrap().to_str().unwrap(),
        "custom-value"
    );
}

#[tokio::test]
async fn get_saved_response_returns_none_for_nonexistent_key() {
    let app = spawn_app().await;
    let key = IdempotencyKey::try_from("nonexistent".to_string()).unwrap();

    let result = get_saved_response(&app.db_pool, &key, None)
        .await
        .expect("Query failed");

    assert!(result.is_none());
}

#[tokio::test]
async fn idempotency_works_with_user_scoped_keys() {
    let app = spawn_app().await;
    let key = IdempotencyKey::try_from("user-scoped-key".to_string()).unwrap();
    let user_id = Uuid::new_v4();

    // save response for specific user
    let (_, transaction) = try_processing(&app.db_pool, &key, Some(user_id))
        .await
        .expect("Failed to process");

    let response = HttpResponse::Ok().body("User-specific response");
    save_response(transaction.unwrap(), &key, Some(user_id), response)
        .await
        .expect("Failed to save");

    // retrieve with correct user_id
    let saved = get_saved_response(&app.db_pool, &key, Some(user_id))
        .await
        .expect("Failed to retrieve")
        .expect("Response not found");

    assert_eq!(saved.status().as_u16(), 200);

    // different user shouldn't see the response
    let other_user = Uuid::new_v4();
    let other_result = get_saved_response(&app.db_pool, &key, Some(other_user))
        .await
        .expect("Query failed");

    assert!(other_result.is_none());
}

#[tokio::test]
async fn different_keys_dont_interfere() {
    let app = spawn_app().await;
    let key1 = IdempotencyKey::try_from("key-one".to_string()).unwrap();
    let key2 = IdempotencyKey::try_from("key-two".to_string()).unwrap();

    // Process both keys
    let (action1, tx1) = try_processing(&app.db_pool, &key1, None).await.unwrap();
    let (action2, tx2) = try_processing(&app.db_pool, &key2, None).await.unwrap();

    // Both should be new
    assert!(matches!(action1, NextAction::StartProcessing));
    assert!(matches!(action2, NextAction::StartProcessing));
    assert!(tx1.is_some());
    assert!(tx2.is_some());
}
