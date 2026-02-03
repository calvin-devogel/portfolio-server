use actix_session::{SessionMiddleware, storage::RedisSessionStore};
// add `middleware::from_fn, web,` once you start creating routes
use actix_web::{App, HttpServer, cookie::Key, dev::Server, web, web::Data};
use actix_web_flash_messages::{FlashMessagesFramework, storage::CookieMessageStore};
use secrecy::{ExposeSecret, SecretString};
use sqlx::{PgPool, postgres::PgPoolOptions};
use std::net::TcpListener;
use tracing_actix_web::TracingLogger;

// use crate::authentication::reject_anonymous_users;
use crate::configuration::{DatabaseSettings, Settings};
use crate::routes::*;
// use crate::routes::*;

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
    pub async fn build(configuration: Settings) -> Result<Self, anyhow::Error> {
        let connection_pool = get_connection_pool(&configuration.database);

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

    pub fn port(&self) -> u16 {
        self.port
    }

    // only return when the application is stopped
    pub async fn run_until_stopped(self) -> Result<(), std::io::Error> {
        self.server.await
    }
}

// run the actual server
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
    let server = HttpServer::new(move || {
        App::new()
            .wrap(message_framework.clone())
            .wrap(SessionMiddleware::new(
                redis_store.clone(),
                secret_key.clone(),
            ))
            .wrap(TracingLogger::default())
            .route("/api/login", web::post().to(login))
            .route("/api/logout", web::post().to(logout))
            // why check-auth?
            .route("/api/check-auth", web::get().to(check_auth))
            .app_data(db_pool.clone())
            .app_data(base_url.clone())
            .app_data(Data::new(HmacSecret(hmac_secret.clone())))
    })
    .listen(listener)?
    .run();

    Ok(server)
}

pub fn get_connection_pool(configuration: &DatabaseSettings) -> PgPool {
    PgPoolOptions::new().connect_lazy_with(configuration.connect_options())
}
