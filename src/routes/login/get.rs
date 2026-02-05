use actix_web::HttpResponse;

use crate::session_state::TypedSession;

// I feel like this should be extended
#[allow(clippy::future_not_send)]
#[tracing::instrument(name = "Check if authenticated", skip(session))]
pub async fn check_auth(session: TypedSession) -> HttpResponse {
    match session.get_user_id() {
        Ok(Some(_)) => HttpResponse::Ok().finish(),
        _ => HttpResponse::Unauthorized().finish(),
    }
}