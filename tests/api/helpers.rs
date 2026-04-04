use argon2::{
    Algorithm, Argon2, Params, PasswordHasher, Version,
    password_hash::{SaltString, rand_core::OsRng},
};
use chrono::{DateTime, Utc};
use reqwest::header::HeaderMap;
use secrecy::{ExposeSecret, SecretString};
use sqlx::{Connection, Executor, PgConnection, PgPool};
use std::sync::LazyLock;
use totp_rs::{Secret, TOTP};
use uuid::Uuid;

use portfolio_server::{
    configuration::{DatabaseSettings, get_configuration},
    startup::{Application, get_connection_pool},
    telemetry::{get_subscriber, init_subscriber},
    types::user::UserRole,
};

// ensure the `tracing` task is only initialized once using `LazyLock`
static TRACING: LazyLock<()> = LazyLock::new(|| {
    let default_filter_level = "info".to_string();
    let subscriber_name = "test".to_string();

    if std::env::var("TEST_LOG").is_ok() {
        let subscriber = get_subscriber(subscriber_name, default_filter_level, std::io::stdout);
        init_subscriber(subscriber);
    } else {
        let subscriber = get_subscriber(subscriber_name, default_filter_level, std::io::sink);
        init_subscriber(subscriber);
    }
});

#[derive(serde::Deserialize, serde::Serialize, Clone, Debug, PartialEq)]
pub struct CarouselImage {
    pub src: String,
    pub alt: Option<String>,
    pub caption: Option<String>,
}

#[derive(serde::Deserialize, serde::Serialize, Clone, Debug, PartialEq)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum ArticleSection {
    Markdown {
        content: String,
    },
    Carousel {
        label: String,
        slides: Vec<CarouselImage>,
    },
}

#[derive(serde::Deserialize, Debug)]
pub struct GetResponse {
    pub data: Vec<ArticleRecord>,
}

#[derive(serde::Deserialize, Clone, Debug)]
pub struct ArticleRecord {
    pub post_id: Uuid,
    pub title: String,
    pub slug: String,
    pub sections: Vec<ArticleSection>,
    pub excerpt: String,
    pub author: String,
    pub published: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(serde::Serialize)]
pub struct PublishRequest {
    pub post_id: Uuid,
    pub published: bool,
}

#[derive(serde::Serialize)]
pub struct EditRequest {
    pub post_id: Uuid,
    pub title: Option<String>,
    pub sections: Option<Vec<serde_json::Value>>,
    pub excerpt: Option<String>,
    pub author: Option<String>,
}

#[derive(Debug, serde::Serialize)]
pub struct TestUser {
    pub user_id: Uuid,
    pub username: String,
    pub password: String,
    pub user_role: UserRole,
}

impl TestUser {
    pub fn generate() -> Self {
        Self {
            user_id: Uuid::new_v4(),
            username: Uuid::new_v4().to_string(),
            password: Uuid::new_v4().to_string(),
            user_role: UserRole::Admin,
        }
    }

    pub async fn login(&self, app: &TestApp) {
        app.post_login(&serde_json::json!({
            "username": &self.username,
            "password": &self.password
        }))
        .await;
    }

    async fn store(&self, pool: &PgPool) {
        let salt = SaltString::generate(&mut OsRng);

        let password_hash = Argon2::new(
            Algorithm::Argon2id,
            Version::V0x13,
            Params::new(15000, 2, 1, None).unwrap(),
        )
        .hash_password(self.password.as_bytes(), &salt)
        .unwrap()
        .to_string();

        sqlx::query!(
            "INSERT INTO users (user_id, username, password_hash, totp_enabled, role)
            VALUES ($1, $2, $3, $4, $5)",
            self.user_id,
            self.username,
            password_hash,
            false,
            self.user_role as UserRole,
        )
        .execute(pool)
        .await
        .expect("Failed to store test user.");
    }

    pub async fn enable_totp(&self, pool: &PgPool) -> TOTP {
        const SECRET_B32: &str = "JBSWY3DPEHPK3PXPJBSWY3DPEHPK3PX";
        const ENCRYPTION_KEY: &[u8; 32] = b"f2e4f32183efde11831c64557303bf22";
        let encrypted = portfolio_server::crypto::encrypt(ENCRYPTION_KEY, SECRET_B32.as_bytes())
            .expect("failed to encrypt TOTP secret");
        sqlx::query!(
            "UPDATE users SET totp_secret = $1, totp_enabled = TRUE WHERE user_id = $2",
            encrypted,
            self.user_id,
        )
        .execute(pool)
        .await
        .expect("Failed to enable TOTP for test user");

        TOTP::new(
            totp_rs::Algorithm::SHA1,
            6,
            1,
            30,
            Secret::Encoded(SECRET_B32.to_string())
                .to_bytes()
                .expect("Invalid test secret"),
            None,
            "test".to_string(),
        )
        .expect("Failed to build test TOTP")
    }
}

pub struct TestApp {
    pub address: String,
    pub db_pool: PgPool,
    pub _port: u16,
    pub test_user: TestUser,
    pub api_client: reqwest::Client,
    pub xsrf_token: String,
}

impl TestApp {
    pub async fn get_home(&self) -> reqwest::Response {
        self.api_client
            .get(&format!("{}/", &self.address))
            .send()
            .await
            .expect("Failed to execute request.")
    }
    pub async fn post_login<Body>(&self, body: &Body) -> reqwest::Response
    where
        Body: serde::Serialize,
    {
        self.api_client
            .post(&format!("{}/v1/login", &self.address))
            .header("X-XSRF-TOKEN", &self.xsrf_token)
            .form(&body)
            .send()
            .await
            .expect("Failed to execute request.")
    }

    pub async fn post_logout(&self) -> reqwest::Response {
        self.api_client
            .post(&format!("{}/v1/logout", &self.address))
            .header("X-XSRF-TOKEN", &self.xsrf_token)
            .send()
            .await
            .expect("Failed to execute request.")
    }

    pub async fn check_auth(&self) -> reqwest::Response {
        self.api_client
            .get(&format!("{}/v1/check_auth", &self.address))
            .send()
            .await
            .expect("Failed to execute request")
    }

    pub async fn generic_request(&self) -> reqwest::Response {
        self.api_client
            .get(&format!("{}/health_check", &self.address))
            .send()
            .await
            .expect("Failed to execute request.")
    }

    pub async fn post_message<Body>(&self, body: &Body) -> reqwest::Response
    where
        Body: serde::Serialize,
    {
        self.api_client
            .post(&format!("{}/v1/contact", &self.address))
            .header("Idempotency-Key", Uuid::new_v4().to_string())
            .header("X-XSRF-TOKEN", &self.xsrf_token)
            .form(&body)
            .send()
            .await
            .expect("Failed to send message.")
    }

    pub async fn get_messages(&self) -> reqwest::Response {
        self.api_client
            .get(&format!("{}/v1/admin/messages", &self.address))
            .send()
            .await
            .expect("Failed to get messages.")
    }

    pub async fn patch_message<Body>(&self, body: &Body) -> reqwest::Response
    where
        Body: serde::Serialize,
    {
        self.api_client
            .patch(&format!("{}/v1/admin/messages", &self.address))
            .header("Idempotency-Key", Uuid::new_v4().to_string())
            .header("X-XSRF-TOKEN", &self.xsrf_token)
            .json(&body)
            .send()
            .await
            .expect("Failed to send message")
    }

    pub async fn patch_message_with_reused_key<Body>(
        &self,
        body: &Body,
        idempotency_key: &Uuid,
    ) -> reqwest::Response
    where
        Body: serde::Serialize,
    {
        self.api_client
            .patch(&format!("{}/v1/admin/messages", &self.address))
            .header("Idempotency-Key", idempotency_key.to_string())
            .header("X-XSRF-TOKEN", &self.xsrf_token)
            .json(&body)
            .send()
            .await
            .expect("Failed to send message")
    }

    pub async fn get_article(&self, on_published: &str, slug: Option<String>) -> reqwest::Response {
        let mut header_map = HeaderMap::new();
        // horrible, just horrible
        header_map.insert("BlogPost-Page", "1".parse().unwrap());
        header_map.insert("BlogPost-Page-Size", "20".parse().unwrap());
        header_map.insert(
            "BlogPost-OnPublished",
            on_published.parse().unwrap_or("false".parse().unwrap()),
        );
        if slug.is_some() {
            header_map.insert("BlogPost-Slug", slug.unwrap().parse().unwrap());
        }
        self.api_client
            .get(&format!("{}/v1/blog", &self.address))
            .headers(header_map)
            .send()
            .await
            .expect("Failed to get blog posts")
    }

    pub async fn post_article<Body>(&self, body: &Body) -> reqwest::Response
    where
        Body: serde::Serialize,
    {
        self.api_client
            .post(format!("{}/v1/admin/blog/post", &self.address))
            .header("Idempotency-Key", Uuid::new_v4().to_string())
            .header("X-XSRF-TOKEN", &self.xsrf_token)
            .json(&body)
            .send()
            .await
            .expect("Failed to post blog article")
    }

    pub async fn publish_article<Body>(&self, body: &Body) -> reqwest::Response
    where
        Body: serde::Serialize,
    {
        self.api_client
            .patch(format!("{}/v1/admin/blog/publish", &self.address))
            .header("Idempotency-Key", Uuid::new_v4().to_string())
            .header("X-XSRF-TOKEN", &self.xsrf_token)
            .json(&body)
            .send()
            .await
            .expect("Failed to publish blog article")
    }

    pub async fn edit_article<Body>(&self, body: &Body) -> reqwest::Response
    where
        Body: serde::Serialize,
    {
        self.api_client
            .patch(format!("{}/v1/admin/blog/edit", &self.address))
            .header("Idempotency-Key", Uuid::new_v4().to_string())
            .header("X-XSRF-TOKEN", &self.xsrf_token)
            .json(&body)
            .send()
            .await
            .expect("Failed to edit blog article")
    }

    pub async fn delete_article<Body>(&self, body: &Body) -> reqwest::Response
    where
        Body: serde::Serialize,
    {
        self.api_client
            .delete(format!("{}/v1/admin/blog/delete", &self.address))
            .header("Idempotency-Key", Uuid::new_v4().to_string())
            .header("X-XSRF-TOKEN", &self.xsrf_token)
            .json(&body)
            .send()
            .await
            .expect("Failed to delete article")
    }

    pub async fn post_verify_totp(&self, code: &str) -> reqwest::Response {
        self.api_client
            .post(&format!("{}/v1/verify_totp", &self.address))
            .header("X-XSRF-TOKEN", &self.xsrf_token)
            .json(&serde_json::json!({ "code": code }))
            .send()
            .await
            .expect("Failed to execute request.")
    }

    pub async fn get_totp_setup(&self) -> reqwest::Response {
        self.api_client
            .get(&format!("{}/v1/admin/totp/setup", &self.address))
            .send()
            .await
            .expect("Failed to execute request.")
    }

    pub async fn post_totp_confirm(&self, code: &str) -> reqwest::Response {
        self.api_client
            .post(&format!("{}/v1/admin/totp/confirm", &self.address))
            .header("X-XSRF-TOKEN", &self.xsrf_token)
            .json(&serde_json::json!({ "code": code }))
            .send()
            .await
            .expect("Failed to execute request.")
    }

    pub async fn post_totp_disable(&self, password: &str) -> reqwest::Response {
        self.api_client
            .post(&format!("{}/v1/admin/totp/disable", &self.address))
            .header("X-XSRF-TOKEN", &self.xsrf_token)
            .json(&serde_json::json!({ "password": password }))
            .send()
            .await
            .expect("Failed to execute request.")
    }

    pub async fn get_totp_status(&self) -> reqwest::Response {
        self.api_client
            .get(&format!("{}/v1/admin/totp/status", &self.address))
            .send()
            .await
            .expect("Failed to execute request")
    }

    pub async fn post_create_user<Body>(&self, body: &Body) -> reqwest::Response
    where
        Body: serde::Serialize,
    {
        self.api_client
            .post(&format!("{}/v1/admin/create_user", &self.address))
            .header("X-XSRF-TOKEN", &self.xsrf_token)
            .header("Idempotency-Key", Uuid::new_v4().to_string())
            .json(&body)
            .send()
            .await
            .expect("Failed to execute request.")
    }

    pub async fn post_change_password<Body>(&self, body: &Body) -> reqwest::Response
    where
        Body: serde::Serialize,
    {
        self.api_client
            .post(&format!("{}/v1/change_password", &self.address))
            .header("X-XSRF-TOKEN", &self.xsrf_token)
            .json(&body)
            .send()
            .await
            .expect("Failed to execute request.")
    }

    pub async fn get_user_names(&self, username: Option<String>) -> reqwest::Response {
        let mut header_map = HeaderMap::new();
        if username.is_some() {
            header_map.insert("UserName", username.unwrap().parse().unwrap());
        }

        self.api_client
            .get(&format!("{}/v1/admin/users", &self.address))
            .headers(header_map)
            .send()
            .await
            .expect("Failed to execute request")
    }

    pub async fn get_chat_token(&self) -> reqwest::Response {
        self.api_client
            .get(&format!("{}/v1/chat_token", &self.address))
            .send()
            .await
            .expect("Failed to get chat token")
    }

    pub async fn patch_user_role<Body>(&self, user_id: &str, body: &Body) -> reqwest::Response
    where
        Body: serde::Serialize,
    {
        self.api_client
            .patch(&format!(
                "{}/v1/admin/users/{}/role",
                &self.address, user_id
            ))
            .header("X-XSRF-TOKEN", &self.xsrf_token)
            .json(&body)
            .send()
            .await
            .expect("Failed to execute request")
    }

    pub async fn post_accept_invitation<Body>(&self, body: &Body) -> reqwest::Response
    where
        Body: serde::Serialize,
    {
        self.api_client
            .post(&format!("{}/v1/accept", &self.address))
            .header("X-XSRF-TOKEN", &self.xsrf_token)
            .json(&body)
            .send()
            .await
            .expect("Failed to execute request")
    }

    pub async fn set_must_change_password(&self, user_id: Uuid) {
        sqlx::query!(
            "UPDATE users SET must_change_password = true WHERE user_id = $1",
            user_id,
        )
        .execute(&self.db_pool)
        .await
        .expect("Failed to set must_change_password");
    }

    pub async fn get_must_change_password_flag(&self, user_id: Uuid) -> bool {
        sqlx::query!(
            "SELECT must_change_password FROM users WHERE user_id = $1",
            user_id,
        )
        .fetch_one(&self.db_pool)
        .await
        .expect("Failed to query must_change_password flag")
        .must_change_password
    }
}

pub async fn spawn_app() -> TestApp {
    LazyLock::force(&TRACING);

    let configuration = {
        let mut c = get_configuration().expect("Failed to read configuration.");
        c.database.database_name = Uuid::new_v4().to_string();
        c.application.port = 0;
        c
    };

    //create and migrate the database
    configure_database(&configuration.database).await;

    // launch as background task
    let application = Application::build(configuration.clone())
        .await
        .expect("Failed to build configuration.");

    let application_port = application.port();
    let _ = tokio::spawn(application.run_until_stopped());

    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .cookie_store(true)
        .build()
        .unwrap();

    let seed = client
        .get(format!("http://localhost:{}/v1/blog", application_port))
        .send()
        .await
        .expect("Failed to seed CSRF token");
    let xsrf_token = seed
        .cookies()
        .find(|c| c.name() == "XSRF-TOKEN")
        .map(|c| c.value().to_string())
        .expect("XSRF-TOKEN not found in seed response");

    let test_app = TestApp {
        address: format!("http://localhost:{}", application_port),
        _port: application_port,
        db_pool: get_connection_pool(&configuration.database),
        test_user: TestUser::generate(),
        api_client: client,
        xsrf_token,
    };
    test_app.test_user.store(&test_app.db_pool).await;
    test_app
}

async fn configure_database(config: &DatabaseSettings) -> PgPool {
    let maintenance_settings = DatabaseSettings {
        database_name: "postgres".to_string(),
        username: "postgres".to_string(),
        password: SecretString::new("password".into()),
        ..config.clone()
    };

    let mut connection = PgConnection::connect_with(&maintenance_settings.connect_options())
        .await
        .expect("Failed to connect to Postgres");
    connection
        .execute(format!(r#"CREATE DATABASE "{}";"#, config.database_name).as_str())
        .await
        .expect("Failed to create database.");

    let connection_pool = PgPool::connect_with(config.connect_options())
        .await
        .expect("Failed to connect to Postgres.");
    sqlx::migrate!("./migrations")
        .run(&connection_pool)
        .await
        .expect("Failed to migrate the database.");

    connection_pool
}

// i need a way to seed a user into the database without exposing the hash explicitly
// there should be a way to do this inside the admin console eventually, but for now
// I'll just compute the hash here and manually insert into the database.
pub fn _seed_user(username: String, password: SecretString) -> TestUser {
    let salt = SaltString::generate(&mut OsRng);
    let password_hash = Argon2::default()
        .hash_password(password.expose_secret().as_bytes(), &salt)
        .unwrap()
        .to_string();

    TestUser {
        user_id: Uuid::new_v4(),
        username,
        password: password_hash,
        user_role: UserRole::Admin,
    }
}
