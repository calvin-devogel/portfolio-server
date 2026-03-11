use secrecy::{ExposeSecret, SecretString};
use serde_aux::field_attributes::deserialize_number_from_string;
use sqlx::postgres::{PgConnectOptions, PgSslMode};

#[derive(Debug)]
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
    pub totp_encryption_key: SecretString,
}

#[derive(serde::Deserialize, Clone)]
pub struct RateLimitSettings {
    #[serde(default = "default_message_rate_limit")]
    pub message: MessageRateLimitSettings,
}

impl Default for RateLimitSettings {
    fn default() -> Self {
        Self {
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

    // A panic here is acceptable. Like the session middleware, the config is a critical
    // component and if it's not configured correctly, the app shouldn't start at all
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
        .build()
        .expect("Failed to build settings");

    settings.try_deserialize::<Settings>()
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn env_as_str() {
        assert_eq!(Environment::Local.as_str(), "local");
        assert_eq!(Environment::Production.as_str(), "production");
    }

    #[test]
    fn env_try_from() {
        assert_eq!(
            Environment::try_from("local".to_string()).unwrap().as_str(),
            "local"
        );
        assert_eq!(
            Environment::try_from("production".to_string())
                .unwrap()
                .as_str(),
            "production"
        );

        assert_eq!(
            Environment::try_from("LOCAL".to_string()).unwrap().as_str(),
            "local"
        );
        assert_eq!(
            Environment::try_from("PRODUCTION".to_string())
                .unwrap()
                .as_str(),
            "production"
        );

        let e = Environment::try_from("invalid_env".to_string()).unwrap_err();
        assert!(e.contains("invalid_env"));
        assert!(e.contains("local") && e.contains("production"));
    }

    #[test]
    fn rate_limit_default() {
        let rate_limit = RateLimitSettings::default();
        assert_eq!(
            rate_limit.message.max_messages,
            default_message_rate_limit().max_messages
        );
        assert_eq!(
            rate_limit.message.window_minutes,
            default_message_rate_limit().window_minutes
        );
    }

    #[test]
    fn db_ssl_settings() {
        let dummy_db_settings = DatabaseSettings {
            username: "test".to_string(),
            password: SecretString::new("test".into()),
            port: 2000,
            host: "test".to_string(),
            database_name: "test".to_string(),
            require_ssl: true,
        };

        let connect_options = dummy_db_settings.connect_options();
        assert!(format!("{connect_options:?}").contains("Require"));

        let connect_options_no_ssl = DatabaseSettings {
            require_ssl: false,
            ..dummy_db_settings
        }
        .connect_options();
        assert!(format!("{connect_options_no_ssl:?}").contains("Prefer"));
    }
}
