use actix_session::{SessionMiddleware, storage::RedisSessionStore};
// add `middleware::from_fn, web,` once you start creating routes
use actix_web::{App, HttpServer, cookie::Key, dev::Server, middleware::from_fn, web, web::Data};
use actix_web_flash_messages::{FlashMessagesFramework, storage::CookieMessageStore};
use actix_web_ratelimit::{RateLimit, config::RateLimitConfig, store::MemoryStore};
use secrecy::{ExposeSecret, SecretString};
use sqlx::{PgPool, postgres::PgPoolOptions};
use std::net::TcpListener;
use std::sync::Arc;
use tracing_actix_web::TracingLogger;

use crate::authentication::reject_anonymous_users;
use crate::configuration::{DatabaseSettings, Settings};
use crate::routes::{check_auth, health_check, login, logout, test_reject};

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

        // sqlx::migrate!("./migrations").run(&connection_pool).await.expect("Failed to run migrations!");

        let address = format!(
            "{}:{}",
            configuration.application.host, configuration.application.port,
        );

        let listener = TcpListener::bind(address)?;
        let port = listener.local_addr().unwrap().port();
        let server = run(
            listener,
            connection_pool,
            configuration.application.base_url,
            configuration.application.hmac_secret,
            configuration.redis_uri,
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
) -> Result<Server, anyhow::Error> {
    let db_pool = Data::new(db_pool);
    let base_url = Data::new(ApplicationBaseUrl(base_url));
    let secret_key = Key::from(hmac_secret.expose_secret().as_bytes());
    let message_store = CookieMessageStore::builder(secret_key.clone()).build();
    let message_framework = FlashMessagesFramework::builder(message_store).build();
    let redis_store = RedisSessionStore::new(redis_uri.expose_secret()).await?;
    let rate_config = RateLimitConfig::default().max_requests(3).window_secs(10);
    let rate_store = Arc::new(MemoryStore::new());
    let server = HttpServer::new(move || {
        App::new()
            .wrap(message_framework.clone())
            .wrap(SessionMiddleware::new(
                redis_store.clone(),
                secret_key.clone(),
            ))
            .wrap(RateLimit::new(rate_config.clone(), rate_store.clone()))
            .wrap(TracingLogger::default())
            // inconsistent - vs _ on heatlh_check and check-auth, fix please
            .route("/health_check", web::get().to(health_check))
            .route("/api/login", web::post().to(login))
            .route("/api/logout", web::post().to(logout))
            .route("/api/check-auth", web::get().to(check_auth))
            .service(
                web::scope("/api/admin")
                    .wrap(from_fn(reject_anonymous_users))
                    .route("/test", web::get().to(test_reject)),
            )
            .app_data(db_pool.clone())
            .app_data(base_url.clone())
            .app_data(Data::new(HmacSecret(hmac_secret.clone())))
    })
    .listen(listener)?
    .run();

    Ok(server)
}

#[must_use]
pub fn get_connection_pool(configuration: &DatabaseSettings) -> PgPool {
    PgPoolOptions::new().connect_lazy_with(configuration.connect_options())
}
