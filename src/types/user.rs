#[derive(serde::Deserialize, Debug, Clone)]
pub enum UserActionType {
    CreateUser,
    UpdateUserInfo,
    ChangePassword,
    DeleteUser,
}

#[derive(PartialEq, Eq, Debug, serde::Serialize)]
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

#[derive(serde::Deserialize, Debug, Clone)]
pub struct UserActionForm {
    pub action_type: UserActionType,
    pub user_id: Option<String>,
    pub payload: serde_json::Value,
}