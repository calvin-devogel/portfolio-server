use secrecy::{ExposeSecret, SecretString};
use serde_aux::field_attributes::deserialize_number_from_string;
use sqlx::postgres::{PgConnectOptions, PgSslMode};

pub enum Environment {
    Local,
    Production,
}

impl Environment {
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::Production => "production",
        }
    }
}

impl TryFrom<String> for Environment {
    type Error = String;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        match s.to_lowercase().as_str() {
            "local" => Ok(Self::Local),
            "production" => Ok(Self::Production),
            other => Err(format!(
                "{other} is not a supported environment. \
                Use either `local` or `production`."
            )),
        }
    }
}

#[derive(serde::Deserialize, Clone)]
pub struct Settings {
    pub database: DatabaseSettings,
    pub application: ApplicationSettings,
    pub redis_uri: SecretString,
    #[serde(default)]
    pub rate_limit: RateLimitSettings,
    pub cors: CorsSettings,
    pub ttl: TtlSettings,
}

#[derive(serde::Deserialize, Clone)]
pub struct ApplicationSettings {
    #[serde(deserialize_with = "deserialize_number_from_string")]
    pub port: u16,
    pub host: String,
    pub base_url: String,
    pub hmac_secret: SecretString,
}

#[derive(serde::Deserialize, Clone)]
pub struct RateLimitSettings {
    #[serde(default = "default_login_rate_limit")]
    pub login: LoginRateLimitSettings,
    #[serde(default = "default_message_rate_limit")]
    pub message: MessageRateLimitSettings,
}

impl Default for RateLimitSettings {
    fn default() -> Self {
        Self {
            login: default_login_rate_limit(),
            message: default_message_rate_limit(),
        }
    }
}

#[derive(serde::Deserialize, Clone)]
pub struct LoginRateLimitSettings {
    #[serde(deserialize_with = "deserialize_number_from_string")]
    pub max_requests: usize,
    #[serde(deserialize_with = "deserialize_number_from_string")]
    pub window_secs: u64,
}

#[derive(serde::Deserialize, Clone)]
pub struct MessageRateLimitSettings {
    #[serde(deserialize_with = "deserialize_number_from_string")]
    pub max_messages: usize,
    #[serde(deserialize_with = "deserialize_number_from_string")]
    pub window_minutes: usize,
}

const fn default_login_rate_limit() -> LoginRateLimitSettings {
    LoginRateLimitSettings {
        max_requests: 3,
        window_secs: 10,
    }
}

const fn default_message_rate_limit() -> MessageRateLimitSettings {
    MessageRateLimitSettings {
        max_messages: 3,
        window_minutes: 60,
    }
}

#[derive(serde::Deserialize, Clone)]
pub struct DatabaseSettings {
    pub username: String,
    pub password: SecretString,
    #[serde(deserialize_with = "deserialize_number_from_string")]
    pub port: u16,
    pub host: String,
    pub database_name: String,
    pub require_ssl: bool,
}

impl DatabaseSettings {
    #[must_use]
    pub fn connect_options(&self) -> PgConnectOptions {
        let ssl_mode = if self.require_ssl {
            PgSslMode::Require
        } else {
            PgSslMode::Prefer
        };
        PgConnectOptions::new()
            .host(&self.host)
            .username(&self.username)
            .password(self.password.expose_secret())
            .port(self.port)
            .ssl_mode(ssl_mode)
            .database(&self.database_name)
    }
}

#[derive(serde::Deserialize, Clone)]
pub struct CorsSettings {
    pub allowed_origins: Vec<String>,
    pub max_age: usize,
}

#[derive(serde::Deserialize, Clone)]
pub struct TtlSettings {
    pub ttl_hours: i64,
    // hey, this isn't referenced anywhere, what's up with that?
    pub idle_timeout_minutes: u32,
}

#[allow(clippy::missing_errors_doc)]
/// # Panics
/// panic gracefully please
pub fn get_configuration() -> Result<Settings, config::ConfigError> {
    let base_path = std::env::current_dir().expect("Failed to determine the current directory");
    let configuration_directory = base_path.join("configuration");

    // detect environment
    let environment: Environment = std::env::var("APP_ENVIRONMENT")
        .unwrap_or_else(|_| "local".into())
        .try_into()
        .expect("Failed to parse APP_ENVIRONMENT");

    let environment_filename = format!("{}.yaml", environment.as_str());
    let settings = config::Config::builder()
        .add_source(config::File::from(
            configuration_directory.join("base.yaml"),
        ))
        .add_source(config::File::from(
            configuration_directory.join(environment_filename),
        ))
        .add_source(
            config::Environment::with_prefix("APP")
                .prefix_separator("_")
                .separator("__"),
        )
        .build()?;

    settings.try_deserialize::<Settings>()
}
