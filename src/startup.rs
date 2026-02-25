use actix_cors::Cors;
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
use tracing_actix_web::TracingLogger;

use crate::{authentication::reject_anonymous_users};
use crate::configuration::{
    CorsSettings, DatabaseSettings, RateLimitSettings, Settings, TtlSettings,
};
use crate::routes::{
    check_auth,
    get_messages,
    get_blog_posts,
    insert_blog_post,
    publish_blog_post,
    delete_blog_post,
    health_check,
    login,
    logout,
    patch_message,
    post_message,
    root,
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
            configuration.rate_limit,
            configuration.cors,
            configuration.ttl,
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
    rate_config: RateLimitSettings,
    cors_config: CorsSettings,
    ttl_config: TtlSettings,
) -> Result<Server, anyhow::Error> {
    let db_pool = Data::new(db_pool);
    let base_url = Data::new(ApplicationBaseUrl(base_url));
    let secret_key = Key::from(hmac_secret.expose_secret().as_bytes());
    let message_store = CookieMessageStore::builder(secret_key.clone())
        .same_site(SameSite::Strict)
        .build();
    let message_framework = FlashMessagesFramework::builder(message_store).build();
    let redis_store = RedisSessionStore::new(redis_uri.expose_secret()).await?;
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
                                ttl_config.ttl_hours,
                            ))
                            .session_ttl_extension_policy(TtlExtensionPolicy::OnEveryRequest),
                    )
                    .build(),
            )
            .wrap(TracingLogger::default())
            .wrap({
                let mut cors = Cors::default();

                for origin in &cors_config.allowed_origins {
                    cors = cors.allowed_origin(origin);
                }

                cors.allowed_methods(vec!["GET", "POST", "PATCH"])
                    .allowed_headers(vec![
                        http::header::AUTHORIZATION,
                        http::header::ACCEPT,
                        http::header::CONTENT_TYPE,
                        http::header::HeaderName::from_static("idempotency-key"),
                    ])
                    .supports_credentials()
                    .max_age(cors_config.max_age)
            })
            .route("/", web::get().to(root))
            .route("/health_check", web::get().to(health_check))
            .route("/api/login", web::post().to(login))
            .route("/api/logout", web::post().to(logout))
            .route("/api/check_auth", web::get().to(check_auth))
            .route("/api/contact", web::post().to(post_message))
            .route("/api/blog", web::get().to(get_blog_posts))
            .service(
                web::scope("/api/admin")
                    .wrap(from_fn(reject_anonymous_users))
                    .route("/messages", web::get().to(get_messages))
                    .route("/messages", web::patch().to(patch_message))
                    .route("/blog", web::post().to(insert_blog_post))
                    .route("/blog", web::patch().to(publish_blog_post))
                    .route("/blog", web::delete().to(delete_blog_post))
            )
            .app_data(db_pool.clone())
            .app_data(base_url.clone())
            .app_data(Data::new(HmacSecret(hmac_secret.clone())))
            .app_data(Data::new(rate_config.message.clone()))
    })
    .listen(listener)?
    .run();

    Ok(server)
}

#[must_use]
pub fn get_connection_pool(configuration: &DatabaseSettings) -> PgPool {
    PgPoolOptions::new().connect_lazy_with(configuration.connect_options())
}
