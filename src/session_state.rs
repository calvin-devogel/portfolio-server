use actix_session::{Session, SessionExt, SessionGetError, SessionInsertError};
use actix_web::{FromRequest, HttpRequest, dev::Payload};
use std::future::{Ready, ready};
use uuid::Uuid;

use crate::types::user::UserRole;

// wrapper type for session
pub struct TypedSession(Session);

#[allow(clippy::missing_errors_doc)]
impl TypedSession {
    const USER_ID_KEY: &'static str = "user_id";
    const MFA_PENDING_KEY: &'static str = "mfa_pending_user_id";
    const USER_ROLE_KEY: &'static str = "user_role";

    pub fn renew(&self) {
        self.0.renew();
    }

    pub fn insert_user_id(&self, user_id: Uuid) -> Result<(), SessionInsertError> {
        self.0.insert(Self::USER_ID_KEY, user_id)
    }

    pub fn get_user_id(&self) -> Result<Option<Uuid>, SessionGetError> {
        self.0.get(Self::USER_ID_KEY)
    }

    pub fn insert_mfa_pending_user_id(&self, user_id: Uuid) -> Result<(), SessionInsertError> {
        self.0.insert(Self::MFA_PENDING_KEY, user_id)
    }

    pub fn get_mfa_pending_user_id(&self) -> Result<Option<Uuid>, SessionGetError> {
        self.0.get(Self::MFA_PENDING_KEY)
    }

    pub fn clear_mfa_pending(&self) {
        self.0.remove(Self::MFA_PENDING_KEY);
    }

    pub fn clear_user_id(&self) {
        self.0.remove(Self::USER_ID_KEY);
    }

    pub fn insert_user_role(&self, role: UserRole) -> Result<(), SessionInsertError> {
        self.0.insert(Self::USER_ROLE_KEY, role.to_string())
    }

    // role should output a role enum
    pub fn get_user_role(&self) -> Result<Option<UserRole>, SessionGetError> {
        match self.0.get::<String>(Self::USER_ROLE_KEY)? {
            Some(role_str) => Ok(role_str.parse::<UserRole>().ok()),
            None => Ok(None),
        }
    }

    pub fn log_out(self) {
        self.0.purge();
    }
}

impl FromRequest for TypedSession {
    // return the same error as Session's implementation of FromRequest
    type Error = <Session as FromRequest>::Error;

    type Future = Ready<Result<Self, Self::Error>>;

    fn from_request(req: &HttpRequest, _payload: &mut Payload) -> Self::Future {
        ready(Ok(Self(req.get_session())))
    }
}
