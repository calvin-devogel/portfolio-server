use actix_limitation::{Limiter};
use actix_session::{SessionMiddleware, storage::RedisSessionStore};
use actix_web::{App, HttpServer, cookie::Key, dev::Server, middleware::from_fn, web, web::Data};
use actix_web_flash_messages::{FlashMessagesFramework, storage::CookieMessageStore};
use secrecy::{ExposeSecret, SecretString};
use sqlx::{PgPool, postgres::PgPoolOptions};
use std::net::TcpListener;
use tracing_actix_web::TracingLogger;

use crate::authentication::reject_anonymous_users;
use crate::configuration::{DatabaseSettings, LoginRateLimitSettings, Settings};
use crate::routes::{
    check_auth,
    health_check,
    login,
    logout,
    test_reject,
    post_message,
    get_messages
};

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

        sqlx::migrate!("./migrations")
            .run(&connection_pool)
            .await?;

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
            configuration.rate_limit.login,
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
    rate_config: LoginRateLimitSettings
) -> Result<Server, anyhow::Error> {
    let db_pool = Data::new(db_pool);
    let base_url = Data::new(ApplicationBaseUrl(base_url));
    let secret_key = Key::from(hmac_secret.expose_secret().as_bytes());
    let message_store = CookieMessageStore::builder(secret_key.clone()).build();
    let message_framework = FlashMessagesFramework::builder(message_store).build();
    let redis_store = RedisSessionStore::new(redis_uri.expose_secret()).await?;
    let limiter = Data::new(
        Limiter::builder(redis_uri.expose_secret())
        .limit(rate_config.max_requests)
        .period(std::time::Duration::from_secs(rate_config.window_secs))
        .build()
        .expect("Failed to build rate limiter")
    );
    let server = HttpServer::new(move || {
        App::new()
            .wrap(message_framework.clone())
            .wrap(SessionMiddleware::new(
                redis_store.clone(),
                secret_key.clone(),
            ))
            .wrap(TracingLogger::default())
            // inconsistent - vs _ on heatlh_check and check-auth, fix please
            .route("/health_check", web::get().to(health_check))
            .route("/api/login", web::post().to(login))
            .route("/api/logout", web::post().to(logout))
            .route("/api/check-auth", web::get().to(check_auth))
            .route("/api/contact", web::post().to(post_message))
            .service(
                web::scope("/api/admin")
                    .wrap(from_fn(reject_anonymous_users))
                    .route("/test", web::get().to(test_reject))
                    .route("/messages", web::get().to(get_messages))
            )
            .app_data(db_pool.clone())
            .app_data(base_url.clone())
            .app_data(Data::new(HmacSecret(hmac_secret.clone())))
            .app_data(limiter.clone())
    })
    .listen(listener)?
    .run();

    Ok(server)
}

#[must_use]
pub fn get_connection_pool(configuration: &DatabaseSettings) -> PgPool {
    PgPoolOptions::new().connect_lazy_with(configuration.connect_options())
}
