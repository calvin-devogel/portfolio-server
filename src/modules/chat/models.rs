// non-semantic names dangit!
// SignalR maps sub to ClaimTypes.NameIdentifier
// sub -> who (UUID)
// exp -> expiry (60 seconds)
// iss -> issuer ("portfolio-server")
// aud -> audience ("portfolio-chat")
#[derive(serde::Serialize, serde::Deserialize)]
pub struct ChatClaims {
    pub name: String,
    pub sub: String,
    pub exp: i64,
    pub iss: String,
    pub aud: String,
}
