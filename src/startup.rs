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

use crate::{
    authentication::reject_anonymous_users,
    configuration::{ CorsSettings, DatabaseSettings, RateLimitSettings, Settings, TtlSettings},
    routes::{
        check_auth,
        delete_blog_post,
        get_blog_posts,
        edit_blog_post,
        publish_blog_post,
        get_messages,
        health_check,
        insert_blog_post,
        login,
        logout,
        patch_message,
        post_message,
        root,
    }
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
            util_config
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
                    .route("/logout", web::post().to(logout))
                    .route("/check_auth", web::get().to(check_auth))
                    .route("/contact", web::post().to(post_message))
                    .route("/blog", web::get().to(get_blog_posts))
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
                            .route("/blog/post", web::post().to(insert_blog_post))
                            .route("/blog/publish", web::patch().to(publish_blog_post))
                            .route("/blog/delete", web::delete().to(delete_blog_post))
                            .route("/blog/edit", web::patch().to(edit_blog_post))
                    ),
            )
            .app_data(db_pool.clone())
            .app_data(base_url.clone())
            .app_data(Data::new(HmacSecret(hmac_secret.clone())))
            .app_data(Data::new(util_config.rate.message.clone()))
    })
    .listen(listener)?
    .run();

    Ok(server)
}

#[must_use]
pub fn get_connection_pool(configuration: &DatabaseSettings) -> PgPool {
    PgPoolOptions::new().connect_lazy_with(configuration.connect_options())
}
