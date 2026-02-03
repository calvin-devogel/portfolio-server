use actix_web::{
    FromRequest, HttpMessage,
    body::MessageBody,
    dev::{ServiceRequest, ServiceResponse},
    error::InternalError,
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

// how would this work considering we're logging in through a front-end?
// this site isn't monolithic so how can we communicate about authentication?
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
