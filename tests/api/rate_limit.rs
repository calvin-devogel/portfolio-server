use std::time::Duration;

use crate::helpers::spawn_app;

#[tokio::test]
async fn test_rate_limit() {
    // arrange
    let app = spawn_app().await;

    // act: make a bunch of rapid-fire requests
    for _ in 0..15 {
        let _res = app.generic_request().await.status();
    }

    // check response on fourth request
    let response = app.generic_request().await;

    assert_eq!(response.status().as_u16(), 429);
}

#[tokio::test]
async fn rate_limit_resets_after_window_passes() {
    // arrange
    let app = spawn_app().await;

    // act 1: make a bunch of rapid-fire requests
    let mut res = app.generic_request().await;
    for _ in 0..9 {
        res = app.generic_request().await;
    }

    assert_eq!(res.status().as_u16(), 429);

    // act 2: wait a few seconds, then make another request
    tokio::time::sleep(Duration::from_secs(10)).await;
    let response = app.generic_request().await;

    // assert
    assert_eq!(response.status().as_u16(), 200);
}
