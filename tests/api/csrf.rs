use crate::helpers::spawn_app;

#[tokio::test]
async fn requests_without_csrf_header_are_rejected() {
    let app = spawn_app().await;

    let response = app
        .api_client
        .post(&format!("{}/v1/login", &app.address))
        .form(&serde_json::json!({ "username": "fake_user", "password": "fake_password"}))
        .send()
        .await
        .expect("Failed to execute request");

    assert_eq!(response.status().as_u16(), 403);
}

#[tokio::test]
async fn requests_with_mismatched_csrf_token_are_rejected() {
    let app = spawn_app().await;

    let response = app
        .api_client
        .post(&format!("{}/v1/login", &app.address))
        .header("X-XSRF-TOKEN", "not-the-right-token")
        .form(&serde_json::json!({ "username": "fake_user", "password": "fake_password" }))
        .send()
        .await
        .expect("Failed to execute request.");

    assert_eq!(response.status().as_u16(), 403);
}
