use crate::authentication::UserId;

#[derive(serde::Deserialize, Debug, Clone)]
pub enum UserActionType {
    CreateUser,
    UpdateUserInfo,
    ChangePassword,
    DeleteUser,
}

#[derive(serde::Deserialize, Debug, Clone)]
pub struct UserActionForm {
    pub action_type: UserActionType,
    pub user_id: Option<UserId>,
    pub payload: serde_json::Value,
}