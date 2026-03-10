use actix_web::{
    FromRequest, HttpMessage,
    body::MessageBody,
    cookie::{Cookie, SameSite},
    dev::{ServiceRequest, ServiceResponse},
    error::InternalError,
    http::Method,
    middleware::Next,
};
use std::ops::Deref;
use uuid::Uuid;

use crate::session_state::TypedSession;
use crate::utils::{e500, unauthorized};

#[derive(Copy, Clone, Debug)]
pub struct UserId(Uuid);

impl std::fmt::Display for UserId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl Deref for UserId {
    type Target = Uuid;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[allow(clippy::future_not_send)]
/// # Errors
/// will return an `actix_web` 500 error if the `user_id` being requested doesn't exist in the database
/// and a 401 if the user trying to access a scoped resource isn't logged in
pub async fn reject_anonymous_users(
    mut req: ServiceRequest,
    next: Next<impl MessageBody>,
) -> Result<ServiceResponse<impl MessageBody>, actix_web::Error> {
    let session = {
        let (http_request, payload) = req.parts_mut();
        TypedSession::from_request(http_request, payload).await
    }?;

    if let Some(user_id) = session.get_user_id().map_err(e500)? {
        req.extensions_mut().insert(UserId(user_id));
        next.call(req).await
    } else {
        let response = unauthorized();
        let e = anyhow::anyhow!("The user has not logged in");
        Err(InternalError::from_response(e, response).into())
    }
}

const XSRF_COOKIE_NAME: &str = "XSRF-TOKEN";
const XSRF_HEADER_NAME: &str = "X-XSRF-TOKEN";

#[allow(clippy::future_not_send)]
pub async fn cross_site_request_forgery_protection(
    request: ServiceRequest,
    next: Next<impl MessageBody>,
) -> Result<ServiceResponse<impl MessageBody>, actix_web::Error> {
    let is_safe = matches!(
        request.method(),
        &Method::GET | &Method::HEAD | &Method::OPTIONS
    );

    if !is_safe {
        let cookie_val = request
            .cookie(XSRF_COOKIE_NAME)
            .map(|c| c.value().to_string());
        let header_val = request
            .headers()
            .get(XSRF_HEADER_NAME)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());

        match (cookie_val, header_val) {
            (Some(c), Some(h)) if !c.is_empty() && c == h => {}
            _ => return Err(actix_web::error::ErrorForbidden("Invalid CSRF token")),
        }
    }

    // reuse the existing token
    // only generate fresh if absent
    let token = request
        .cookie(XSRF_COOKIE_NAME)
        .map(|c| c.value().to_string())
        .unwrap_or_else(|| Uuid::new_v4().to_string());

    let mut res = next.call(request).await?;

    // NOT http_only intentionally, Angular must be able to read this
    let cookie = Cookie::build(XSRF_COOKIE_NAME, token)
        .path("/")
        .secure(true)
        .same_site(SameSite::Strict)
        .finish();

    res.response_mut()
        .add_cookie(&cookie)
        .map_err(actix_web::error::ErrorInternalServerError)?;

    Ok(res)
}
