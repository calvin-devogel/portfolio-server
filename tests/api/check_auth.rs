use crate::helpers::spawn_app;
use portfolio_server::{
    authentication::{Credentials, change_password, validate_credentials_with_verifier},
    errors::AuthError,
};
use secrecy::ExposeSecret;

#[tokio::test]
async fn unauthorized_users_are_unauthorized() {
    // arrange
    let app = spawn_app().await;

    let credentials = serde_json::json!({
        "username": "fake-username",
        "password": "fake-password",
    });

    // act: attempt to log in with fake credentials
    app.post_login(&credentials).await;
    let response = app.check_auth().await;

    // assert
    assert_eq!(response.status().as_u16(), 401);
}

#[tokio::test]
async fn authorized_users_are_authorized() {
    // arrange
    let app = spawn_app().await;
    app.test_user.login(&app).await;

    // act
    let response = app.check_auth().await;

    // assert
    assert_eq!(response.status().as_u16(), 200);
}

// we need to use a dummy verify_password_hash to simulate a hash collision
// or the error state on `user_id.ok_or_else(|| anyhow::anyhow!("Unknown username")))
// will be practically unreachable
#[tokio::test]
async fn unknown_users_are_rejected_on_hash_collisions() {
    let app = spawn_app().await;

    let fake_credentials = Credentials {
        username: "fake_username".to_string(),
        password: secrecy::SecretString::new("fake_password".into()),
    };

    let result =
        validate_credentials_with_verifier(fake_credentials, &app.db_pool, |_, _| Ok(())).await;

    assert!(matches!(result, Err(AuthError::InvalidCredentials(_))));
}

#[tokio::test]
async fn change_password_works() {
    let app = spawn_app().await;

    let user_id = app.test_user.user_id;
    let new_password = secrecy::SecretString::new("new_password".into());

    let _ = change_password(user_id, new_password.clone(), &app.db_pool).await;

    let login_body = serde_json::json!({
        "username": app.test_user.username,
        "password": new_password.expose_secret(),
    });

    let result = app.post_login(&login_body).await;

    assert_eq!(result.status().as_u16(), 200);
}
