use email_address::EmailAddress;

#[derive(serde::Deserialize, Debug, Clone)]
pub enum UserActionType {
    CreateUser,
    UpdateUserInfo,
    ChangePassword,
    DeleteUser,
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
