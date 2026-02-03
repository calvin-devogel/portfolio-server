use actix_web::{HttpResponse, error::InternalError, web};
// use actix_web_flash_messages::FlashMessage;
use crate::authentication::{AuthError, Credentials, validate_credentials};
use crate::session_state::TypedSession;
use secrecy::SecretString;
use sqlx::PgPool;

#[derive(serde::Deserialize, Debug)]
pub struct LoginRequest {
    username: String,
    password: SecretString,
}

#[tracing::instrument(
    skip(request, pool, session),
    fields(username=tracing::field::Empty, user_id=tracing::field::Empty)
)]
pub async fn login(
    request: web::Form<LoginRequest>,
    pool: web::Data<PgPool>,
    session: TypedSession,
) -> Result<HttpResponse, InternalError<AuthError>> {
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
                .map_err(|e| login_redirect(AuthError::UnexpectedError(e.into())))?;

            Ok(HttpResponse::Ok().finish())
        }
        Err(e) => {
            let e = match e {
                AuthError::InvalidCredentials(_) => AuthError::InvalidCredentials(e.into()),
                AuthError::UnexpectedError(_) => AuthError::UnexpectedError(e.into()),
            };
            Err(login_redirect(e))
        }
    }
}

pub async fn logout(session: TypedSession) -> Result<HttpResponse, actix_web::Error> {
    session.log_out();
    Ok(HttpResponse::Ok().finish())
}

// hmmm....
#[tracing::instrument(name = "Check if authenticated", skip(session))]
pub async fn check_auth(session: TypedSession) -> HttpResponse {
    match session.get_user_id() {
        Ok(Some(_)) => HttpResponse::Ok().finish(),
        _ => HttpResponse::Unauthorized().finish(),
    }
}

fn login_redirect(e: AuthError) -> InternalError<AuthError> {
    InternalError::from_response(e, HttpResponse::Unauthorized().finish())
}
