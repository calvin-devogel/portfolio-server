use secrecy::SecretString;

#[derive(serde::Deserialize, Debug)]
pub struct LoginRequest {
    pub username: String,
    pub password: SecretString,
}