use chrono::{DateTime, Utc};
use color_eyre::Result;
use serde::{Deserialize, Serialize};
use sqlx::{
    prelude::{FromRow, Type},
    PgPool,
};
use utoipa::ToSchema;
use uuid::Uuid;

#[derive(Debug, Type, Eq, Hash, PartialEq, Deserialize, Serialize, Clone, ToSchema)]
#[sqlx(type_name = "provider_type", rename_all = "snake_case")]
pub enum ProviderType {
    DockerLocal,
    AWSFargate,
}

#[derive(Debug, FromRow, Serialize, ToSchema)]
pub struct Service {
    pub id: Uuid,
    pub name: String,
    pub provider_type: ProviderType,
    pub details: serde_json::Value,
    pub is_enabled: bool,
    pub descale_at: Option<DateTime<Utc>>,
}

#[derive(Debug, FromRow, Serialize, ToSchema)]
pub struct ServiceWithTopics {
    pub id: Uuid,
    pub name: String,
    pub provider_type: ProviderType,
    pub details: serde_json::Value,
    pub is_enabled: bool,
    pub descale_at: Option<DateTime<Utc>>,
    pub topics: Vec<String>,
}

impl ServiceWithTopics {
    pub fn to_service(&self) -> Service {
        Service {
            id: self.id,
            name: self.name.clone(),
            provider_type: self.provider_type.clone(),
            details: self.details.clone(),
            is_enabled: self.is_enabled,
            descale_at: self.descale_at,
        }
    }
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
            SELECT id, name, provider_type as "provider_type!: ProviderType", details, is_enabled, descale_at
            FROM services
            "#
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(services)
    }

    pub async fn get_services_with_topics(&self) -> Result<Vec<ServiceWithTopics>, sqlx::Error> {
        let services = sqlx::query_as!(
            ServiceWithTopics,
            r#"
            SELECT 
                s.id, 
                s.name, 
                s.provider_type as "provider_type!: ProviderType", 
                s.details, 
                s.is_enabled, 
                s.descale_at, 
                COALESCE(array_agg(t.name) FILTER (WHERE t.name IS NOT NULL), '{}') as "topics!: Vec<String>"
            FROM services as s
            LEFT JOIN service_topics as st ON s.id = st.service_id
            LEFT JOIN topics as t ON st.topic_id = t.id
            GROUP BY s.id
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
            RETURNING id, name, provider_type as "provider_type!: ProviderType", details, is_enabled, descale_at
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
            SELECT id, name, provider_type as "provider_type!: ProviderType", details, is_enabled, descale_at
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
            RETURNING id, name, provider_type as "provider_type!: ProviderType", details, is_enabled, descale_at
            "#,
            service_id
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(service)
    }

    pub async fn get_services_enabled_by_topic(
        &self,
        topic: &str,
    ) -> Result<Vec<Service>, sqlx::Error> {
        let services = sqlx::query_as!(
            Service,
            r#"
            SELECT id, name, provider_type as "provider_type!: ProviderType", details, is_enabled, descale_at
            FROM services as s
            JOIN service_topics as st ON s.id = st.service_id
            WHERE st.topic_id = (
                SELECT id FROM topics WHERE name = $1
            ) AND s.is_enabled = TRUE
            "#,
            topic
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(services)
    }

    pub async fn set_descale_deadlines(
        &self,
        service_ids: &Vec<Uuid>,
        deadline: DateTime<Utc>,
    ) -> Result<(), sqlx::Error> {
        let _ = sqlx::query!(
            r#"
            UPDATE services
            SET descale_at = $2
            WHERE id = ANY($1)
            "#,
            service_ids,
            deadline
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn get_services_with_descale_deadline_reached(
        &self,
    ) -> Result<Vec<Service>, sqlx::Error> {
        let services = sqlx::query_as!(
            Service,
            r#"
            SELECT id, name, provider_type as "provider_type!: ProviderType", details, is_enabled, descale_at
            FROM services
            WHERE descale_at <= NOW()
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(services)
    }

    pub async fn clear_descale_deadline(&self, service_id: &Uuid) -> Result<Service, sqlx::Error> {
        let service = sqlx::query_as!(
            Service,
            r#"
            UPDATE services
            SET descale_at = NULL
            WHERE id = $1
            RETURNING id, name, provider_type as "provider_type!: ProviderType", details, is_enabled, descale_at
            "#,
            service_id,
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(service)
    }

    pub async fn update_service_set_enabled(
        &self,
        service_id: &Uuid,
        is_enabled: &bool,
    ) -> Result<Service, sqlx::Error> {
        let service = sqlx::query_as!(
            Service,
            r#"
            UPDATE services
            SET is_enabled = $2
            WHERE id = $1
            RETURNING id, name, provider_type as "provider_type!: ProviderType", details, is_enabled, descale_at
            "#,
            service_id,
            is_enabled
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(service)
    }
}
