use actix_web::{HttpResponse, ResponseError, dev::ConnectionInfo, error::InternalError, web};
use secrecy::SecretString;
use sqlx::PgPool;

use crate::authentication::{Credentials, validate_credentials};
use crate::errors::AuthError;
use crate::session_state::TypedSession;
use crate::configuration::LoginLimiter;

#[derive(serde::Deserialize, Debug)]
pub struct LoginRequest {
    username: String,
    password: SecretString,
}

#[allow(clippy::missing_errors_doc)]
#[allow(clippy::future_not_send)]
#[tracing::instrument(
    skip(pool, session, limiter),
    fields(username=tracing::field::Empty, user_id=tracing::field::Empty)
)]
pub async fn login(
    conn: ConnectionInfo,
    request: web::Form<LoginRequest>,
    pool: web::Data<PgPool>,
    session: TypedSession,
    limiter: web::Data<LoginLimiter>,
) -> Result<HttpResponse, InternalError<AuthError>> {
    let ip = conn.realip_remote_addr().unwrap_or("unknown").to_string();
    limiter.0
        .count(ip)
        .await
        .map_err(|_| login_error(AuthError::RateLimitExceeded))?;
    
    let credentials = Credentials {
        username: request.username.clone(),
        password: request.password.clone(),
    };

    tracing::Span::current().record("username", tracing::field::display(&credentials.username));

    match validate_credentials(credentials, &pool).await {
        Ok((user_id, totp_enabled)) => {
            tracing::Span::current().record("user_id", tracing::field::display(&user_id));
            session.renew();

            if totp_enabled {
                session
                    .insert_mfa_pending_user_id(user_id)
                    .map_err(|e| login_error(AuthError::UnexpectedError(e.into())))?;

                Ok(HttpResponse::Accepted().json(serde_json::json!({ "mfa_required": true })))
            } else {
                session
                    .insert_user_id(user_id)
                    .map_err(|e| login_error(AuthError::UnexpectedError(e.into())))?;
                Ok(HttpResponse::Ok().finish())
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
