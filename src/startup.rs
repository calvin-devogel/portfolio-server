use actix_cors::Cors;
use actix_session::{
    SessionMiddleware,
    config::{PersistentSession, TtlExtensionPolicy},
    storage::RedisSessionStore,
};
use actix_web::{
    App, HttpResponse, HttpServer,
    cookie::{Key, SameSite},
    dev::Server,
    http,
    middleware::from_fn,
    web::{self, Data},
};
use actix_web_flash_messages::{FlashMessagesFramework, storage::CookieMessageStore};
use secrecy::{ExposeSecret, SecretString};
use sqlx::{PgPool, postgres::PgPoolOptions};
use std::net::TcpListener;
use tracing_actix_web::TracingLogger;

use crate::{
    authentication::{cross_site_request_forgery_protection, reject_anonymous_users, reject_non_admin},
    configuration::{CorsSettings, DatabaseSettings, RateLimitSettings, Settings, TtlSettings},
    routes::{
        chat_token, check_auth, delete_article, edit_article, get_articles, get_messages,
        health_check, insert_article, login, logout, patch_message, post_message, publish_article,
        root, totp_confirm, totp_disable, totp_setup, totp_status, verify_totp, create_user, accept_invitation
    },
};

#[derive(serde::Deserialize, Clone)]
struct UtilConfig {
    rate: RateLimitSettings,
    cors: CorsSettings,
    ttl: TtlSettings,
}

#[derive(Clone)]
struct SecretsConfig {
    hmac: HmacSecret,
    totp: TotpEncryptionKey,
    jwt: JwtPrivateKey,
}

// wrapper type for SecretString
#[derive(Clone)]
pub struct HmacSecret(pub SecretString);

#[derive(Clone)]
pub struct TotpEncryptionKey(pub [u8; 32]);

#[derive(Clone)]
pub struct JwtPrivateKey(pub SecretString);

// wrapper for application url
pub struct ApplicationBaseUrl(pub String);

pub struct Application {
    port: u16,
    server: Server,
}

impl Application {
    #[tracing::instrument(
        name = "Application::build",
        level = "info",
        skip(configuration),
        fields(
            host = %configuration.application.host,
            port = %configuration.application.port,
            db_host = %configuration.database.host,
        )
    )]
    #[allow(clippy::missing_errors_doc)]
    /// # Panics
    /// probably not a bad idea to handle port binding issues gracefully
    pub async fn build(configuration: Settings) -> Result<Self, anyhow::Error> {
        let connection_pool = get_connection_pool(&configuration.database);

        tracing::info!("Database connection pool configured (lazy)");

        connection_pool.acquire().await.map_err(|e| {
            tracing::error!(
                error.cause_chain = ?e,
                error.message = %e,
                "Failed to acquire initial database connection"
            );
            anyhow::anyhow!("Database connectivity check failed: {e}")
        })?;
        tracing::info!("Database connectivity verified");

        let address = format!(
            "{}:{}",
            configuration.application.host, configuration.application.port,
        );

        // reduce run's argument count!
        let util_config = UtilConfig {
            rate: configuration.rate_limit,
            cors: configuration.cors,
            ttl: configuration.ttl,
        };

        let hmac_key = HmacSecret(configuration.application.hmac_secret);
        let raw_totp_key = configuration
            .application
            .totp_encryption_key
            .expose_secret()
            .as_bytes();
        let key: [u8; 32] = raw_totp_key.try_into().map_err(|_| {
            tracing::error!(
                key_len = raw_totp_key.len(),
                "totp_encryption_key is not exactly 32 bytes"
            );
            anyhow::anyhow!("totp_encryption_key must be exactly 32 bytes")
        })?;
        let totp_key = TotpEncryptionKey(key);
        let jwt_key_pem = std::fs::read_to_string(&configuration.application.jwt_private_key_path)
            .map_err(|e| {
                anyhow::anyhow!(
                    "Failed to read jwt_private_key_path '{}': {e}",
                    configuration.application.jwt_private_key_path
                )
            })?;

        let jwt_private_key = JwtPrivateKey(SecretString::from(jwt_key_pem));

        let secrets_config = SecretsConfig {
            hmac: hmac_key,
            totp: totp_key,
            jwt: jwt_private_key,
        };

        let listener = TcpListener::bind(&address).map_err(|e| {
            tracing::error!(
                address = %address,
                error.cause_chain = ?e,
                error.message = %e,
                "Failed to bind TCP listener"
            );
            anyhow::Error::from(e)
        })?;
        tracing::info!(address = %address, "TCP listener bound");
        let port = listener.local_addr().unwrap().port();
        let server = run(
            listener,
            connection_pool,
            configuration.application.base_url,
            secrets_config,
            configuration.redis_uri,
            util_config,
        )
        .await
        .map_err(|e| {
            tracing::error!(
                error.cause_chain = ?e,
                error.message = %e,
                "Server component initialization failed"
            );
            e
        })?;
        tracing::info!("Server components initialized successfully");

        Ok(Self { port, server })
    }

    #[must_use]
    pub const fn port(&self) -> u16 {
        self.port
    }

    #[allow(clippy::missing_errors_doc)]
    // only return when the application is stopped
    pub async fn run_until_stopped(self) -> Result<(), std::io::Error> {
        self.server.await
    }
}

// run the actual server
#[tracing::instrument(name = "Application::run", level = "info", skip_all)]
#[allow(clippy::missing_errors_doc, clippy::too_many_lines)]
async fn run(
    listener: TcpListener,
    db_pool: PgPool,
    base_url: String,
    secrets: SecretsConfig,
    redis_uri: SecretString,
    util_config: UtilConfig,
) -> Result<Server, anyhow::Error> {
    let db_pool = Data::new(db_pool);
    let base_url = Data::new(ApplicationBaseUrl(base_url));
    let secret_key = Key::from(secrets.hmac.0.expose_secret().as_bytes());
    let message_store = CookieMessageStore::builder(secret_key.clone())
        .same_site(SameSite::Strict)
        .build();
    let message_framework = FlashMessagesFramework::builder(message_store).build();

    tracing::info!("Connecting to Redis session store...");
    let redis_store = RedisSessionStore::new(redis_uri.expose_secret())
        .await
        .map_err(|e| {
            tracing::error!(
                error.cause_chain = ?e,
                error.message = %e,
                "Failed to connect to Redis session store"
            );
            anyhow::anyhow!("Redis session store connection failed: {e}")
        })?;
    tracing::info!("Redis session store connected");

    let server = HttpServer::new(move || {
        App::new()
            .wrap(message_framework.clone())
            .wrap(TracingLogger::default())
            .route("/", web::get().to(root))
            .route("/health_check", web::get().to(health_check))
            .service(
                web::scope("/api")
                    .wrap(from_fn(cross_site_request_forgery_protection))
                    .wrap(
                        SessionMiddleware::builder(redis_store.clone(), secret_key.clone())
                            .cookie_same_site(SameSite::Strict)
                            .cookie_http_only(true)
                            .cookie_secure(true)
                            .session_lifecycle(
                                PersistentSession::default()
                                    .session_ttl(actix_web::cookie::time::Duration::hours(
                                        util_config.ttl.ttl_hours,
                                    ))
                                    .session_ttl_extension_policy(
                                        TtlExtensionPolicy::OnEveryRequest,
                                    ),
                            )
                            .build(),
                    )
                    .wrap({
                        let mut cors = Cors::default();

                        for origin in &util_config.cors.allowed_origins {
                            cors = cors.allowed_origin(origin);
                        }

                        cors.allowed_methods(vec!["GET", "POST"])
                            .allowed_headers(vec![
                                http::header::AUTHORIZATION,
                                http::header::ACCEPT,
                                http::header::CONTENT_TYPE,
                                http::header::HeaderName::from_static("idempotency-key"),
                                http::header::HeaderName::from_static("x-xsrf-token"),
                            ])
                            .supports_credentials()
                            .max_age(util_config.cors.max_age)
                    })
                    .route("/login", web::post().to(login))
                    .route("/verify_totp", web::post().to(verify_totp))
                    .route("/logout", web::post().to(logout))
                    .route("/check_auth", web::get().to(check_auth))
                    .route("/contact", web::post().to(post_message))
                    .route("/blog", web::get().to(get_articles))
                    .route("/accept", web::post().to(accept_invitation))
                    .service(
                        web::scope("/chat_token")
                            .wrap(from_fn(reject_anonymous_users))
                            // UserId needs to implement FromRequest?
                            .route("", web::get().to(chat_token)),
                    )
                    .service(
                        web::scope("/admin")
                            .app_data(web::JsonConfig::default().limit(65_536).error_handler(
                                |err, _req| {
                                    actix_web::error::InternalError::from_response(
                                        err,
                                        HttpResponse::PayloadTooLarge().finish(),
                                    )
                                    .into()
                                },
                            ))
                            .wrap({
                                let mut cors = Cors::default();

                                for origin in &util_config.cors.allowed_origins {
                                    cors = cors.allowed_origin(origin);
                                }

                                cors.allowed_methods(vec!["GET", "POST", "PATCH", "DELETE"])
                                    .allowed_headers(vec![
                                        http::header::AUTHORIZATION,
                                        http::header::ACCEPT,
                                        http::header::CONTENT_TYPE,
                                        http::header::HeaderName::from_static("idempotency-key"),
                                        http::header::HeaderName::from_static("x-xsrf-token"),
                                    ])
                                    .supports_credentials()
                                    .max_age(util_config.cors.max_age)
                            })
                            .wrap(from_fn(reject_anonymous_users))
                            .wrap(from_fn(reject_non_admin))
                            .route("/create_user", web::post().to(create_user))
                            .route("/messages", web::get().to(get_messages))
                            .route("/messages", web::patch().to(patch_message))
                            .route("/blog/post", web::post().to(insert_article))
                            .route("/blog/publish", web::patch().to(publish_article))
                            .route("/blog/delete", web::delete().to(delete_article))
                            .route("/blog/edit", web::patch().to(edit_article))
                            .route("/totp/setup", web::get().to(totp_setup))
                            .route("/totp/confirm", web::post().to(totp_confirm))
                            .route("/totp/disable", web::post().to(totp_disable))
                            .route("/totp/status", web::get().to(totp_status)),
                    ),
            )
            .app_data(db_pool.clone())
            .app_data(base_url.clone())
            .app_data(Data::new(secrets.hmac.clone()))
            .app_data(Data::new(util_config.rate.message.clone()))
            .app_data(Data::new(secrets.totp.clone()))
            .app_data(Data::new(secrets.jwt.clone()))
    })
    .listen(listener)?
    .run();

    Ok(server)
}

#[must_use]
pub fn get_connection_pool(configuration: &DatabaseSettings) -> PgPool {
    PgPoolOptions::new().connect_lazy_with(configuration.connect_options())
}
