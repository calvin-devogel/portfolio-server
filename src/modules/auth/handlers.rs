use actix_web::{HttpResponse, ResponseError, dev::ConnectionInfo, error::InternalError, web};
use sqlx::PgPool;

use crate::errors::AuthError;

use super::models::Credentials;
use super::crypto::validate_credentials;
use super::session::TypedSession;

#[allow(clippy::missing_errors_doc)]
#[allow(clippy::future_not_send)]
#[tracing::instrument(
    skip(pool, session),
    fields(username=tracing::field::Empty, user_id=tracing::field::Empty)
)]
pub async fn login(
    _conn: ConnectionInfo,
    request: web::Form<Credentials>,
    pool: web::Data<PgPool>,
    session: TypedSession,
) -> Result<HttpResponse, InternalError<AuthError>> {
    let credentials = request.into_inner();

    tracing::Span::current().record("username", tracing::field::display(&credentials.username));

    match validate_credentials(credentials, &pool).await {
        Ok((user_id, totp_enabled, must_change_password, user_role)) => {
            tracing::Span::current().record("user_id", tracing::field::display(&user_id));
            session.renew();

            if totp_enabled {
                session.clear_user_id();
                session
                    .insert_mfa_pending_user_id(user_id)
                    .map_err(|e| login_error(AuthError::UnexpectedError(e.into())))?;

                Ok(HttpResponse::Accepted().json(serde_json::json!({ "mfa_required": true })))
            } else {
                session
                    .insert_user_id(user_id)
                    .map_err(|e| login_error(AuthError::UnexpectedError(e.into())))?;
                session
                    .insert_user_role(user_role)
                    .map_err(|e| login_error(AuthError::UnexpectedError(e.into())))?;

                if must_change_password {
                    Ok(
                        HttpResponse::Ok()
                            .json(serde_json::json!({ "must_change_password": true })),
                    )
                } else {
                    Ok(HttpResponse::Ok().finish())
                }
            }
        }
        Err(e) => {
            let e = match e {
                AuthError::RateLimitExceeded => AuthError::RateLimitExceeded,
                AuthError::InvalidCredentials(_) => AuthError::InvalidCredentials(e.into()),
                AuthError::UnexpectedError(_) => AuthError::UnexpectedError(e.into()),
            };
            Err(login_error(e))
        }
    }
    
}

#[allow(clippy::missing_errors_doc)]
#[allow(clippy::future_not_send)]
pub async fn logout(session: TypedSession) -> Result<HttpResponse, actix_web::Error> {
    session.log_out();
    Ok(HttpResponse::Ok().finish())
}

fn login_error(e: AuthError) -> InternalError<AuthError> {
    let response = HttpResponse::build(e.status_code()).finish();
    InternalError::from_response(e, response)
}

// I feel like this should be extended
#[allow(clippy::future_not_send)]
#[tracing::instrument(name = "Check if authenticated", skip(session))]
pub async fn check_auth(session: TypedSession) -> HttpResponse {
    match session.get_user_id() {
        Ok(Some(_)) => {
            // renew session on each check_auth to extend TTL
            session.renew();
            let user_role = session.get_user_role();
            match user_role {
                Ok(Some(role)) => HttpResponse::Ok().json(role.to_string()),
                _ => HttpResponse::Unauthorized().finish(),
            }
        }
        _ => HttpResponse::Unauthorized().finish(),
    }
}