use crate::helpers::spawn_app;
use std::sync::Arc;

#[tokio::test]
async fn login_rate_limit_prevents_brute_force() {
    let app = Arc::new(spawn_app().await);
    let bad_login = serde_json::json!({
        "username": app.test_user.username,
        "password": "wrong_password"
    });

    // run requests sequentially with no delay
    let response1 = app.post_login(&bad_login).await;
    println!("Request 1: {}", response1.status());

    let response2 = app.post_login(&bad_login).await;
    println!("Request 2: {}", response2.status());

    let response3 = app.post_login(&bad_login).await;
    println!("Request 3: {}", response3.status());

    assert_eq!(response3.status().as_u16(), 429);
}
