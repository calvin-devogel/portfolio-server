use actix_cors::Cors;
use actix_limitation::Limiter;
use actix_session::{
    SessionMiddleware,
    config::{PersistentSession, TtlExtensionPolicy},
    storage::RedisSessionStore,
};
use actix_web::{
    App, HttpServer,
    cookie::{Key, SameSite},
    dev::Server,
    http,
    middleware::from_fn,
    web,
    web::Data,
};
use actix_web_flash_messages::{FlashMessagesFramework, storage::CookieMessageStore};
use secrecy::{ExposeSecret, SecretString};
use sqlx::{PgPool, postgres::PgPoolOptions};
use std::net::TcpListener;
use std::time::Duration;
use tracing_actix_web::TracingLogger;

use crate::{
    authentication::{reject_anonymous_users, cross_site_request_forgery_protection},
    configuration::{CorsSettings, DatabaseSettings, RateLimitSettings, Settings, TtlSettings, TotpLimiter, LoginLimiter},
    routes::{
        check_auth, delete_article, edit_article, get_articles, get_messages, health_check,
        insert_article, login, logout, patch_message, post_message, publish_article, root,
        totp_confirm, totp_disable, totp_setup, totp_status, verify_totp,
    },
};

#[derive(serde::Deserialize, Clone)]
struct UtilConfig {
    rate: RateLimitSettings,
    cors: CorsSettings,
    ttl: TtlSettings,
}

// wrapper type for SecretString
#[derive(Clone)]
pub struct HmacSecret(pub SecretString);

// wrapper for application url
pub struct ApplicationBaseUrl(pub String);

pub struct Application {
    port: u16,
    server: Server,
}

impl Application {
    #[allow(clippy::missing_errors_doc)]
    /// # Panics
    /// probably not a bad idea to handle port binding issues gracefully
    pub async fn build(configuration: Settings) -> Result<Self, anyhow::Error> {
        let connection_pool = get_connection_pool(&configuration.database);

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

        let listener = TcpListener::bind(address)?;
        let port = listener.local_addr().unwrap().port();
        let server = run(
            listener,
            connection_pool,
            configuration.application.base_url,
            configuration.application.hmac_secret,
            configuration.redis_uri,
            util_config,
        )
        .await?;

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
#[allow(clippy::missing_errors_doc)]
async fn run(
    listener: TcpListener,
    db_pool: PgPool,
    base_url: String,
    hmac_secret: SecretString,
    redis_uri: SecretString,
    util_config: UtilConfig,
) -> Result<Server, anyhow::Error> {
    let db_pool = Data::new(db_pool);
    let base_url = Data::new(ApplicationBaseUrl(base_url));
    let secret_key = Key::from(hmac_secret.expose_secret().as_bytes());
    let message_store = CookieMessageStore::builder(secret_key.clone())
        .same_site(SameSite::Strict)
        .build();
    let message_framework = FlashMessagesFramework::builder(message_store).build();
    let redis_store = RedisSessionStore::new(redis_uri.expose_secret()).await?;
    let login_limiter = Limiter::builder(redis_uri.expose_secret())
        .limit(util_config.rate.login.max_requests)
        .period(Duration::from_secs(util_config.rate.login.window_secs))
        .build()
        .map_err(|e| anyhow::anyhow!("Failed to build login rate limiter: {e}"))?;
    let totp_limiter = Limiter::builder(redis_uri.expose_secret())
        .limit(util_config.rate.totp.max_requests)
        .period(Duration::from_secs(util_config.rate.totp.window_secs))
        .build()
        .map_err(|e| anyhow::anyhow!("Failed to build TOTP rate limiter: {e}"))?;

    let server = HttpServer::new(move || {
        App::new()
            .wrap(message_framework.clone())
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
                            .session_ttl_extension_policy(TtlExtensionPolicy::OnEveryRequest),
                    )
                    .build(),
            )
            .wrap(TracingLogger::default())
            .route("/", web::get().to(root))
            .route("/health_check", web::get().to(health_check))
            .service(
                web::scope("/api")
                    .wrap(from_fn(cross_site_request_forgery_protection))
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
                    .service(
                        web::scope("/admin")
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
            .app_data(Data::new(HmacSecret(hmac_secret.clone())))
            .app_data(Data::new(util_config.rate.message.clone()))
            .app_data(Data::new(LoginLimiter(login_limiter.clone())))
            .app_data(Data::new(TotpLimiter(totp_limiter.clone())))
    })
    .listen(listener)?
    .run();

    Ok(server)
}

#[must_use]
pub fn get_connection_pool(configuration: &DatabaseSettings) -> PgPool {
    PgPoolOptions::new().connect_lazy_with(configuration.connect_options())
}
