use uuid::Uuid;

use crate::helpers::spawn_app;

#[tokio::test]
async fn admin_can_create_invitations_and_they_can_be_accepted() {
    let app = spawn_app().await;
    app.test_user.login(&app).await;

    // create invitation
    let new_user = serde_json::json!({
        "email": "new-user@example.com",
        "role": "User",
    });

    let response = app.post_create_user(&new_user).await;
    assert_eq!(
        response.status().as_u16(),
        200,
        "Invitation response should be 200 OK"
    );

    let response_body: serde_json::Value = response
        .json()
        .await
        .expect("Response should be valid JSON");
    assert_eq!(response_body["success"], true);

    let link = response_body["link"]
        .as_str()
        .expect("Link should be present");
    let token = link
        .split("token=")
        .last()
        .expect("Token should be extractable from link");

    app.post_logout().await;

    // accept
    let accept_payload = serde_json::json!({
        "token": token,
        "username": "new-username",
        "password": "SecurePassword123!",
    });

    let accept_response = app.post_accept_invitation(&accept_payload).await;
    assert_eq!(
        accept_response.status().as_u16(),
        200,
        "Accept invitation response should be 200 OK"
    );

    // ensure the new user can log in
    let new_user_login = serde_json::json!({
        "username": accept_payload["username"],
        "password": accept_payload["password"],
    });

    let login_response = app.post_login(&new_user_login).await;
    assert_eq!(
        login_response.status().as_u16(),
        200,
        "New user should be able to log in after accepting invitation"
    );
}

#[tokio::test]
async fn admin_can_change_user_roles() {
    let app = spawn_app().await;

    // create and extract invitation
    let new_user = serde_json::json!({
        "email": "role-test@example.com",
        "role": "User",
    });

    let create_response = app.post_create_user(&new_user).await;
    let create_body: serde_json::Value = create_response
        .json()
        .await
        .expect("Response should be valid JSON");
    let token = create_body["link"]
        .as_str()
        .unwrap()
        .split("token=")
        .last()
        .unwrap();

    let accept_payload = serde_json::json!({
        "token": token,
        "username": "role-test-user",
        "password": "SecurePassword123!",
    });

    app.post_logout().await;
    app.post_accept_invitation(&accept_payload).await;

    app.test_user.login(&app).await;

    let user_record = sqlx::query!(
        "SELECT user_id FROM users WHERE username = $1",
        accept_payload["username"].to_string()
    )
    .fetch_one(&app.db_pool)
    .await
    .expect("Failed to fetch accepted user");

    let user_id = user_record.user_id.to_string();

    let role_update = serde_json::json!({ "role": "admin" });
    let response = app.patch_user_role(&user_id, &role_update).await;
    assert_eq!(response.status().as_u16(), 200);
}

#[tokio::test]
async fn anonymous_users_cannot_change_user_roles() {
    let app = spawn_app().await;

    let role_update = serde_json::json!({ "role": "admin" });
    let response = app
        .patch_user_role(Uuid::new_v4().to_string().as_str(), &role_update)
        .await;
    assert_eq!(response.status().as_u16(), 401);
}

#[tokio::test]
async fn anonymous_users_cannot_create_invitations() {
    let app = spawn_app().await;

    let new_user = serde_json::json!({
        "email": "test@email.com",
        "role": "User",
    });

    let response = app.post_create_user(&new_user).await;
    assert_eq!(response.status().as_u16(), 401);
}

#[tokio::test]
async fn invalid_invitations_are_rejected() {
    let app = spawn_app().await;

    let accept_payload = serde_json::json!({
        "token": "invalid-or-fake-token-12345",
        "username": "fake-user",
        "password": "FakePassword123!",
    });

    let accept_response = app.post_accept_invitation(&accept_payload).await;
    assert_eq!(accept_response.status().as_u16(), 400);
}

#[tokio::test]
async fn used_invitations_cannot_be_reused() {
    let app = spawn_app().await;
    app.test_user.login(&app).await;

    let new_user = serde_json::json!({
        "email": "reuse-test@example.com",
        "role": "User"
    });
    let create_response = app.post_create_user(&new_user).await;
    let create_body: serde_json::Value = create_response.json().await.unwrap();
    let token = create_body["link"]
        .as_str()
        .unwrap()
        .split("token=")
        .last()
        .unwrap();

    let accept_payload = serde_json::json!({
        "token": token,
        "username": "reuse-test-user",
        "password": "SecurePassword123!",
    });

    let first_accept = app.post_accept_invitation(&accept_payload).await;
    assert_eq!(first_accept.status().as_u16(), 200);

    let second_accept = app.post_accept_invitation(&accept_payload).await;
    assert_eq!(second_accept.status().as_u16(), 400);
}

#[tokio::test]
async fn passwords_can_be_changed() {
    let app = spawn_app().await;
    app.test_user.login(&app).await;

    let new_password = Uuid::new_v4().to_string();

    let old_credentials = serde_json::json!({
        "username": app.test_user.username,
        "password": app.test_user.password,
    });

    let credentials = serde_json::json!({
        "username": app.test_user.username,
        "password": app.test_user.password,
        "new_password": new_password,
    });

    let new_credentials = serde_json::json!({
        "username": app.test_user.username,
        "password": new_password,
    });

    let response = app.post_change_password(&credentials).await;
    assert_eq!(response.status().as_u16(), 202);
    app.post_logout().await;

    let response = app.post_login(&old_credentials).await;
    assert_eq!(response.status().as_u16(), 401);

    let response = app.post_login(&new_credentials).await;
    assert_eq!(response.status().as_u16(), 200);
}

#[tokio::test]
async fn usernames_can_be_queried() {
    let app = spawn_app().await;
    app.test_user.login(&app).await;

    let response = app.get_user_names(None).await;
    assert_eq!(response.status().as_u16(), 200);
    assert!(
        response
            .text()
            .await
            .unwrap()
            .contains(&app.test_user.username)
    );
}

#[tokio::test]
async fn anonymous_users_cannot_query_usernames() {
    let app = spawn_app().await;

    let response = app.get_user_names(None).await;
    assert_eq!(response.status().as_u16(), 401);
}

#[tokio::test]
async fn users_can_be_queried_by_username() {
    let app = spawn_app().await;
    app.test_user.login(&app).await;

    let response = app
        .get_user_names(Some(app.test_user.username.clone()))
        .await;
    assert_eq!(response.status().as_u16(), 200);
    assert!(
        response
            .text()
            .await
            .unwrap()
            .contains(&app.test_user.username)
    );
}
