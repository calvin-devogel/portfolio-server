use email_address::EmailAddress;

#[derive(serde::Deserialize, Debug, Clone)]
pub enum UserActionType {
    CreateUser,
    UpdateUserInfo,
    ChangePassword,
    DeleteUser,
}

#[derive(PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize, sqlx::Type)]
#[sqlx(type_name = "user_role", rename_all = "lowercase")]
pub enum UserRole {
    Admin,
    User,
    ChatUser
}

impl UserRole {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "admin" => Some(UserRole::Admin),
            "user" => Some(UserRole::User),
            "chat_user" => Some(UserRole::ChatUser),
            _ => None,
        }
    }
}

impl ToString for UserRole {
    fn to_string(&self) -> String {
        match self {
            UserRole::Admin => "admin".to_string(),
            UserRole::User => "user".to_string(),
            UserRole::ChatUser => "chat_user".to_string(),
        }
    }
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