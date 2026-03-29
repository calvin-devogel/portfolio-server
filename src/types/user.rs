#[derive(serde::Deserialize, Debug, Clone)]
pub enum UserActionType {
    CreateUser,
    UpdateUserInfo,
    ChangePassword,
    DeleteUser,
}

#[derive(PartialEq, Eq)]
pub enum UserRole {
    Admin,
    ChatUser
}

impl UserRole {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "Admin" => Some(UserRole::Admin),
            "ChatUser" => Some(UserRole::ChatUser),
            _ => None,
        }
    }
}

impl ToString for UserRole {
    fn to_string(&self) -> String {
        match self {
            UserRole::Admin => "Admin".to_string(),
            UserRole::ChatUser => "ChatUser".to_string(),
        }
    }
}

#[derive(serde::Deserialize, Debug, Clone)]
pub struct UserActionForm {
    pub action_type: UserActionType,
    pub user_id: Option<String>,
    pub payload: serde_json::Value,
}