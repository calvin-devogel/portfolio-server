use argon2::{
    Algorithm, Argon2, Params, PasswordHasher, Version,
    password_hash::{SaltString, rand_core::OsRng},
};
use secrecy::{ExposeSecret, SecretString};
use sqlx::{Connection, Executor, PgConnection, PgPool};
use std::sync::LazyLock;
use uuid::Uuid;

use portfolio_server::{
    configuration::{DatabaseSettings, get_configuration},
    startup::{Application, get_connection_pool},
    telemetry::{get_subscriber, init_subscriber},
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

#[derive(Debug, serde::Serialize)]
pub struct TestUser {
    pub user_id: Uuid,
    pub username: String,
    pub password: String,
}

impl TestUser {
    pub fn generate() -> Self {
        Self {
            user_id: Uuid::new_v4(),
            username: Uuid::new_v4().to_string(),
            password: Uuid::new_v4().to_string(),
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
            "INSERT INTO users (user_id, username, password_hash)
            VALUES ($1, $2, $3)",
            self.user_id,
            self.username,
            password_hash,
        )
        .execute(pool)
        .await
        .expect("Failed to store test user.");
    }
}

pub struct TestApp {
    pub address: String,
    pub db_pool: PgPool,
    pub _port: u16,
    pub test_user: TestUser,
    pub api_client: reqwest::Client,
}

impl TestApp {
    pub async fn post_login<Body>(&self, body: &Body) -> reqwest::Response
    where
        Body: serde::Serialize,
    {
        self.api_client
            .post(&format!("{}/api/login", &self.address))
            .form(&body)
            .send()
            .await
            .expect("Failed to execute request.")
    }

    pub async fn post_logout(&self) -> reqwest::Response {
        self.api_client
            .post(&format!("{}/api/logout", &self.address))
            .send()
            .await
            .expect("Failed to execute request.")
    }

    pub async fn check_auth(&self) -> reqwest::Response {
        self.api_client
            .get(&format!("{}/api/check_auth", &self.address))
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
            .post(&format!("{}/api/contact", &self.address))
            .header("Idempotency-Key", Uuid::new_v4().to_string())
            .form(&body)
            .send()
            .await
            .expect("Failed to send message.")
    }

    pub async fn get_messages(&self) -> reqwest::Response {
        self.api_client
            .get(&format!("{}/api/admin/messages", &self.address))
            .send()
            .await
            .expect("Failed to get messages.")
    }

    pub async fn patch_message<Body>(&self, body: &Body) -> reqwest::Response
    where
        Body: serde::Serialize,
    {
        self.api_client
            .patch(&format!("{}/api/admin/messages", &self.address))
            .header("Idempotency-Key", Uuid::new_v4().to_string())
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
            .patch(&format!("{}/api/admin/messages", &self.address))
            .header("Idempotency-Key", idempotency_key.to_string())
            .json(&body)
            .send()
            .await
            .expect("Failed to send message")
    }

    pub async fn get_blog(&self) -> reqwest::Response {
        self.api_client
            .get(&format!("{}/api/blog", &self.address))
            .send()
            .await
            .expect("Failed to get blog posts")
    }

    pub async fn post_blog<Body>(&self, body: &Body) -> reqwest::Response
    where
        Body: serde::Serialize,
    {
        self.api_client
            .post(format!("{}/api/admin/blog", &self.address))
            .header("Idempotency-Key", Uuid::new_v4().to_string())
            .json(&body)
            .send()
            .await
            .expect("Failed to post blog entry")
    }

    pub async fn patch_blog<Body>(&self, body: &Body) -> reqwest::Response
    where
        Body: serde::Serialize,
    {
        self.api_client
            .patch(format!("{}/api/admin/blog", &self.address))
            .header("Idempotency-Key", Uuid::new_v4().to_string())
            .json(&body)
            .send()
            .await
            .expect("Failed to patch blog entry")
    }

    pub async fn delete_blog<Body>(&self, body: &Body) -> reqwest::Response
    where
        Body: serde::Serialize,
    {
        self.api_client
            .delete(format!("{}/api/admin/blog", &self.address))
            .header("Idempotency-Key", Uuid::new_v4().to_string())
            .json(&body)
            .send()
            .await
            .expect("Failed to delete blog")
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

    let test_app = TestApp {
        address: format!("http://localhost:{}", application_port),
        _port: application_port,
        db_pool: get_connection_pool(&configuration.database),
        test_user: TestUser::generate(),
        api_client: client,
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
    }
}
