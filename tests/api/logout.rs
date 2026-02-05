use crate::helpers::spawn_app;

#[tokio::test]
async fn logout_clears_session_state() {
    // arrange
    let app = spawn_app().await;

    // act 1: login
    app.test_user.login(&app).await;
    // assert_eq!(api_login_response.status().as_u16(), 200);

    //act 2: check auth should succeed
    let auth_response = app.check_auth().await;
    assert_eq!(auth_response.status().as_u16(), 200);

    // act 3: logout
    let logout_response = app.post_logout().await;
    assert_eq!(logout_response.status().as_u16(), 200);

    // act 4: check auth should fail
    let auth_response_after = app.check_auth().await;
    assert_eq!(auth_response_after.status().as_u16(), 401);
}
