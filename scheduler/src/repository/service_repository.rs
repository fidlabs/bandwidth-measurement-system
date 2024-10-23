use color_eyre::Result;
use serde::{Deserialize, Serialize};
use sqlx::{
    prelude::{FromRow, Type},
    PgPool,
};
use uuid::Uuid;

#[derive(Debug, Type, Eq, Hash, PartialEq, Deserialize, Serialize)]
#[sqlx(type_name = "provider_type", rename_all = "snake_case")]
pub enum ProviderType {
    DockerLocal,
    AWSFargate,
}

#[derive(Debug, FromRow, Serialize)]
pub struct Service {
    pub id: Uuid,
    pub name: String,
    pub provider_type: ProviderType,
    pub details: serde_json::Value,
    pub is_enabled: bool,
}

pub struct ServiceRepository {
    pool: PgPool,
}

impl ServiceRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn get_services(&self) -> Result<Vec<Service>, sqlx::Error> {
        let services = sqlx::query_as!(
            Service,
            r#"
            SELECT id, name, provider_type as "provider_type!: ProviderType", details, is_enabled
            FROM services
            "#
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(services)
    }

    pub async fn create_service(
        &self,
        name: &str,
        provider_type: ProviderType,
        details: &serde_json::Value,
    ) -> Result<Service, sqlx::Error> {
        let service = sqlx::query_as!(
            Service,
            r#"
            INSERT INTO services (name, provider_type, details, is_enabled)
            VALUES ($1, $2, $3, $4)
            RETURNING id, name, provider_type as "provider_type!: ProviderType", details, is_enabled
            "#,
            name,
            provider_type as ProviderType,
            details,
            true
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(service)
    }

    pub async fn get_service_by_id(&self, service_id: &Uuid) -> Result<Service, sqlx::Error> {
        let service = sqlx::query_as!(
            Service,
            r#"
            SELECT id, name, provider_type as "provider_type!: ProviderType", details, is_enabled
            FROM services
            WHERE id = $1
            "#,
            service_id
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(service)
    }

    pub async fn delete_service_by_id(&self, service_id: &Uuid) -> Result<Service, sqlx::Error> {
        let service = sqlx::query_as!(
            Service,
            r#"
            DELETE FROM services
            WHERE id = $1
            RETURNING id, name, provider_type as "provider_type!: ProviderType", details, is_enabled
            "#,
            service_id
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(service)
    }
}
