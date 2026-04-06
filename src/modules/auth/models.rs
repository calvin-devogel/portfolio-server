use actix_web::{FromRequest, HttpMessage, dev::Payload};
use email_address::EmailAddress;
use secrecy::SecretString;
use std::future::{Ready, ready};
use std::ops::Deref;
use uuid::Uuid;

#[derive(serde::Deserialize, Debug, Clone)]
pub enum UserActionType {
    CreateUser,
    UpdateUserInfo,
    ChangePassword,
    DeleteUser,
}

#[derive(serde::Deserialize, Debug)]
pub struct Credentials {
    pub username: String,
    pub password: SecretString,
}

pub type StoredCredentials = (Uuid, SecretString, bool, bool, UserRole);
pub type UserDetails = (Uuid, bool, bool, UserRole);

#[derive(Copy, Clone, Debug)]
pub struct UserId(pub Uuid);

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

impl FromRequest for UserId {
    type Error = actix_web::Error;
    type Future = Ready<Result<Self, Self::Error>>;

    fn from_request(req: &actix_web::HttpRequest, _: &mut Payload) -> Self::Future {
        ready(
            req.extensions()
                .get::<UserId>()
                .copied()
                .ok_or_else(|| actix_web::error::ErrorUnauthorized("Not Authenticated")),
        )
    }
}

#[derive(PartialEq, Eq, Debug, Clone, Copy, serde::Serialize, serde::Deserialize, sqlx::Type)]
#[sqlx(type_name = "user_role", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum UserRole {
    Admin,
    User,
    ChatUser,
}

impl std::str::FromStr for UserRole {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "admin" => Ok(UserRole::Admin),
            "user" => Ok(UserRole::User),
            "chat_user" => Ok(UserRole::ChatUser),
            _ => Err(()),
        }
    }
}

impl std::fmt::Display for UserRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UserRole::Admin => write!(f, "admin"),
            UserRole::User => write!(f, "user"),
            UserRole::ChatUser => write!(f, "chat_user"),
        }
    }
}

pub struct TotpQuery {
    pub secret: Vec<u8>,
    pub role: UserRole,
    pub must_change_password: bool,
}

#[derive(serde::Deserialize, Debug)]
pub struct TotpRequest {
    pub code: String,
}

#[derive(Clone)]
pub struct TotpEncryptionKey(pub [u8; 32]);

#[derive(serde::Deserialize, Debug)]
pub struct DisableTotpRequest {
    pub password: SecretString,
}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct CreateUser {
    pub email: String,
    pub role: UserRole,
}

impl CreateUser {
    pub fn validate(&self) -> Result<(), actix_web::Error> {
        if !EmailAddress::is_valid(&self.email) {
            return Err(actix_web::error::ErrorBadRequest("Invalid email address"));
        }
        Ok(())
    }
}

#[derive(serde::Deserialize, Debug, Clone)]
pub struct UserActionForm {
    pub action_type: UserActionType,
    pub user_id: Option<String>,
    pub payload: serde_json::Value,
}

#[derive(serde::Serialize, Debug)]
pub struct User {
    pub user_id: String,
    pub username: String,
    pub role: String,
    pub must_change_password: bool,
}

#[derive(serde::Deserialize)]
pub struct RoleUpdate {
    pub role: UserRole,
}

#[derive(serde::Deserialize)]
pub struct ChangePasswordBody {
    pub current_password: SecretString,
    pub new_password: SecretString,
}

#[derive(serde::Deserialize)]
pub struct AcceptInvitationParams {
    pub token: String,
    pub username: String,
    pub password: String, // add a validator to both front and backend
}
