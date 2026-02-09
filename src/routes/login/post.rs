use actix_limitation::Limiter;
use actix_web::{HttpResponse, ResponseError, error::InternalError, web};
use secrecy::SecretString;
use sqlx::PgPool;

use crate::authentication::{AuthError, Credentials, validate_credentials};
use crate::session_state::TypedSession;

#[derive(serde::Deserialize, Debug)]
pub struct LoginRequest {
    username: String,
    password: SecretString,
}

#[allow(clippy::missing_errors_doc)]
#[allow(clippy::future_not_send)]
#[tracing::instrument(
    skip(request, pool, session),
    fields(username=tracing::field::Empty, user_id=tracing::field::Empty)
)]
pub async fn login(
    request: web::Form<LoginRequest>,
    pool: web::Data<PgPool>,
    session: TypedSession,
    limiter: web::Data<Limiter>,
) -> Result<HttpResponse, InternalError<AuthError>> {
    let rate_limit_key = format!("login:{}", request.username);

    match limiter.count(rate_limit_key).await {
        Ok(result) if result.remaining() == 0 => {
            return Err(login_error(AuthError::RateLimitExceeded));
        }
        Ok(_) => {}
        Err(e) => {
            tracing::error!("Rate limiter error: {e:?}");
        }
    }

    let credentials = Credentials {
        username: request.username.clone(),
        password: request.password.clone(),
    };

    tracing::Span::current().record("username", tracing::field::display(&credentials.username));

    match validate_credentials(credentials, &pool).await {
        Ok(user_id) => {
            tracing::Span::current().record("user_id", tracing::field::display(&user_id));
            session.renew();
            session
                .insert_user_id(user_id)
                .map_err(|e| login_error(AuthError::UnexpectedError(e.into())))?;

            Ok(HttpResponse::Ok().finish())
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

#[allow(clippy::future_not_send, clippy::missing_errors_doc)]
pub async fn logout(session: TypedSession) -> Result<HttpResponse, actix_web::Error> {
    session.log_out();
    Ok(HttpResponse::Ok().finish())
}

fn login_error(e: AuthError) -> InternalError<AuthError> {
    let response = HttpResponse::build(e.status_code()).finish();
    InternalError::from_response(e, response)
}
