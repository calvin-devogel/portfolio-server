use actix_web::{
    FromRequest,
    body::MessageBody,
    cookie::{Cookie, SameSite},
    dev::{ServiceRequest, ServiceResponse},
    http::Method,
    middleware::Next,
};
use uuid::Uuid;

use crate::core::error::AuthError;
use crate::modules::auth::{TypedSession, UserRole};

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

    let is_authenticated = session
        .get_user_id()
        .map_err(|e| AuthError::Unexpected(e.into()))?
        .is_some();

    if is_authenticated {
        next.call(req).await
    } else {
        tracing::warn!(
            "Unauthenticated user attempted to access protected route: {}",
            req.path()
        );
        Err(AuthError::Unauthorized("The user has not logged in".to_string()).into())
    }
}

#[allow(clippy::future_not_send)]
pub async fn reject_non_admin(
    mut req: ServiceRequest,
    next: Next<impl MessageBody>,
) -> Result<ServiceResponse<impl MessageBody>, actix_web::Error> {
    let session = {
        let (http_request, payload) = req.parts_mut();
        TypedSession::from_request(http_request, payload).await
    };

    let session = session.expect("session middleware not configured");

    if let Some(user_role) = session
        .get_user_role()
        .map_err(|e| AuthError::Unexpected(e.into()))?
        && user_role == UserRole::Admin
    {
        return next.call(req).await;
    }

    let user_id = session
        .get_user_id()
        .map_err(|e| AuthError::Unexpected(e.into()))?;

    if user_id.is_some() {
        tracing::warn!(
            "Authenticated non-admin user attempted to access admin route: {}",
            req.path()
        );
        return Err(AuthError::Forbidden(
            "The user does not have permission to access this resource".to_string(),
        )
        .into());
    }

    tracing::warn!(
        "Unauthenticated user attempted to access admin route: {}",
        req.path()
    );
    Err(AuthError::Unauthorized("The user has not logged in".to_string()).into())
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
    
    let csrf_exempt = ["/v1/web_vitals"];
    let is_exempt = csrf_exempt.iter().any(|p| req.path() == *p);

    if !is_safe && !is_exempt {
        let cookie_val = req.cookie(XSRF_COOKIE_NAME).map(|c| c.value().to_string());
        let header_val = req
            .headers()
            .get(XSRF_HEADER_NAME)
            .and_then(|v| v.to_str().ok())
            .map(&str::to_string);

        match (cookie_val, header_val) {
            (Some(c), Some(h)) if !c.is_empty() && c == h => {}
            _ => {
                tracing::warn!(
                    "CSRF token validation failed on unsafe request to {}",
                    req.path()
                );
                return Err(AuthError::Forbidden("Invalid CSRF token".to_string()).into());
            }
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
