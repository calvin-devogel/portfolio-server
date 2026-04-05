use actix_web::{
    FromRequest, HttpMessage,
    body::MessageBody,
    cookie::{Cookie, SameSite},
    dev::{ServiceRequest, ServiceResponse},
    error::InternalError,
    http::Method,
    middleware::Next,
};
use uuid::Uuid;

use crate::session_state::TypedSession;
use crate::types::user::UserRole;
use crate::utils::{e500, unauthorized};
use crate::modules::auth::UserId;

const XSRF_COOKIE_NAME: &str = "XSRF-TOKEN";
const XSRF_HEADER_NAME: &str = "X-XSRF-TOKEN";

#[allow(clippy::future_not_send)]
pub async fn reject_unauthenticated(
    mut req: ServiceRequest,
    next: Next<impl MessageBody>,
) -> Result<ServiceResponse<impl MessageBody>, actix_web::Error> {
    let session = {
        let (http_request, payload) = req.parts_mut();
        TypedSession::from_request(http_request, payload).await
    };

    // SAFETY: TypedSession::from_request always returns Ok(). If the session middleware
    // isn't configured, get_session() will panic since the middleware is a critical
    // component.
    let session = session.expect("session middleware not configured");

    // SAFETY: A panic here is correct behavior. insert_user_id only accepts UUID,
    // so the stored value will always be deserializable as Uuid, and calling unwrap()
    // on get_user_id is acceptable. A panic here is in effect, equivalent to the
    // session middleware not being configured.
    if let Some(user_id) = session.get_user_id().map_err(e500)? {
        req.extensions_mut().insert(UserId(user_id));
        next.call(req).await
    } else {
        let response = unauthorized();
        let e = anyhow::anyhow!("The user has not logged in");
        Err(InternalError::from_response(e, response).into())
    }
}

pub async fn reject_non_admin(
    mut req: ServiceRequest,
    next: Next<impl MessageBody>,
) -> Result<ServiceResponse<impl MessageBody>, actix_web::Error> {
    let session = {
        let (http_request, payload) = req.parts_mut();
        TypedSession::from_request(http_request, payload).await
    };

    let session = session.expect("session middleware not configured");

    if let Some(user_role) = session.get_user_role().map_err(e500)?
        && user_role == UserRole::Admin
    {
        return next.call(req).await;
    }

    let response = unauthorized();
    let e = anyhow::anyhow!("The user does not have admin privileges");
    Err(InternalError::from_response(e, response).into())
}

#[allow(clippy::future_not_send)]
pub async fn csrf_protection(
    req: ServiceRequest,
    next: Next<impl MessageBody>,
) -> Result<ServiceResponse<impl MessageBody>, actix_web::Error> {
    let is_safe = matches!(
        req.method(),
        &Method::GET | &Method::HEAD | &Method::OPTIONS
    );

    if !is_safe {
        let cookie_val = req
            .cookie(XSRF_COOKIE_NAME)
            .map(|c| c.value().to_string());
        let header_val = req
            .headers()
            .get(XSRF_HEADER_NAME)
            .and_then(|v| v.to_str().ok())
            .map(&str::to_string);

        match (cookie_val, header_val) {
            (Some(c), Some(h)) if !c.is_empty() && c == h => {}
            _ => return Err(actix_web::error::ErrorForbidden("Invalid CSRF token")),
        }
    }

    // reuse the existing token,
    // only generate fresh if absent
    let token = req
        .cookie(XSRF_COOKIE_NAME)
        .map_or_else(|| Uuid::new_v4().to_string(), |c| c.value().to_string());

    let mut res = next.call(req).await?;
    
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
