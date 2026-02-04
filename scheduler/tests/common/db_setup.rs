// scheduler/tests/common/db_setup.rs

use sqlx::PgPool;

pub struct TestDatabase {
    pub pool: PgPool,
    pub name: String,
    admin_pool: PgPool,
}

impl TestDatabase {
    pub async fn new(postgres_port: u16) -> Self {
        // Generate unique name from test function
        let test_name = std::thread::current()
            .name()
            .unwrap_or("unknown")
            .rsplit("::")
            .next()
            .unwrap_or("unknown")
            .to_lowercase()
            .replace(|c: char| !c.is_alphanumeric(), "_");

        let db_name = format!("test_{}", test_name);

        // Connect to postgres database to create test database
        let admin_url = format!(
            "postgres://postgres:postgres@127.0.0.1:{}/postgres",
            postgres_port
        );
        let admin_pool = PgPool::connect(&admin_url)
            .await
            .expect("Failed to connect to Postgres admin");

        // Drop if exists, then create
        sqlx::query(&format!("DROP DATABASE IF EXISTS \"{}\"", db_name))
            .execute(&admin_pool)
            .await
            .expect("Failed to drop test database");

        sqlx::query(&format!("CREATE DATABASE \"{}\"", db_name))
            .execute(&admin_pool)
            .await
            .expect("Failed to create test database");

        // Connect to new database
        let db_url = format!(
            "postgres://postgres:postgres@127.0.0.1:{}/{}",
            postgres_port, db_name
        );
        let pool = PgPool::connect(&db_url)
            .await
            .expect("Failed to connect to test database");

        // Run migrations
        sqlx::migrate!("./src/migrations")
            .run(&pool)
            .await
            .expect("Failed to run migrations");

        Self {
            pool,
            name: db_name,
            admin_pool,
        }
    }

    pub fn url(&self, port: u16) -> String {
        format!(
            "postgres://postgres:postgres@127.0.0.1:{}/{}",
            port, self.name
        )
    }
}

impl Drop for TestDatabase {
    fn drop(&mut self) {
        if std::env::var("KEEP_TEST_DB").is_ok() {
            println!("Keeping test database: {}", self.name);
            return;
        }
        // Note: Actual cleanup happens when pool is dropped
        // Database will be dropped on next test run via DROP IF EXISTS
    }
}

// Seed functions
pub async fn seed_service(
    pool: &PgPool,
    name: &str,
    location: &str,
    provider_type: &str,
) -> uuid::Uuid {
    let record = sqlx::query_scalar::<_, uuid::Uuid>(
        r#"
        INSERT INTO services (name, provider_type, details, is_enabled, location)
        VALUES ($1, $2::provider_type, '{}', true, $3)
        RETURNING id
        "#,
    )
    .bind(name)
    .bind(provider_type)
    .bind(location)
    .fetch_one(pool)
    .await
    .expect("Failed to seed service");

    record
}

pub async fn seed_topic(pool: &PgPool, name: &str) -> i32 {
    let record = sqlx::query!(
        r#"
        INSERT INTO topics (name)
        VALUES ($1)
        ON CONFLICT (name) DO UPDATE SET name = $1
        RETURNING id
        "#,
        name
    )
    .fetch_one(pool)
    .await
    .expect("Failed to seed topic");

    record.id
}

pub async fn seed_service_topic(pool: &PgPool, service_id: uuid::Uuid, topic_id: i32) {
    sqlx::query!(
        r#"
        INSERT INTO service_topics (service_id, topic_id)
        VALUES ($1, $2)
        ON CONFLICT DO NOTHING
        "#,
        service_id,
        topic_id
    )
    .execute(pool)
    .await
    .expect("Failed to seed service_topic");
}

/// Helper to seed a service with its topics
pub async fn seed_service_with_topics(
    pool: &PgPool,
    name: &str,
    location: &str,
    provider_type: &str,
    topics: Vec<&str>,
) -> uuid::Uuid {
    let service_id = seed_service(pool, name, location, provider_type).await;

    for topic_name in topics {
        let topic_id = seed_topic(pool, topic_name).await;
        seed_service_topic(pool, service_id, topic_id).await;
    }

    service_id
}
