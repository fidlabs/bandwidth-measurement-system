use color_eyre::Result;
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Clone)]
pub struct TopicRepository {
    pool: PgPool,
}

impl TopicRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn upsert_worker_topics(
        &self,
        worker_name: &String,
        topics: Vec<String>,
    ) -> Result<(), sqlx::Error> {
        // Insert new topics if they don't exist
        sqlx::query!(
            r#"
            INSERT INTO topics (name)
            SELECT * FROM unnest($1::text[])
            ON CONFLICT (name)
            DO NOTHING
            "#,
            &topics
        )
        .execute(&self.pool)
        .await?;

        // Insert new worker and topic relation if they don't exist
        sqlx::query!(
            r#"
            INSERT INTO worker_topics (worker_name, topic_id)
            SELECT $1, id FROM (
                SELECT id FROM topics WHERE name = ANY($2::text[])
            ) AS topic_ids
            ON CONFLICT (worker_name, topic_id)
            DO NOTHING
            "#,
            worker_name,
            &topics
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn remove_worker_topics(&self, worker_name: &String) -> Result<(), sqlx::Error> {
        // Remove worker and topic relation
        sqlx::query!(
            r#"
            DELETE FROM worker_topics
            WHERE worker_name = $1
            "#,
            worker_name
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn upsert_service_topics(
        &self,
        service_id: &Uuid,
        topics: &Vec<String>,
    ) -> Result<(), sqlx::Error> {
        // Insert new topics if they don't exist
        sqlx::query!(
            r#"
            INSERT INTO topics (name)
            SELECT * FROM unnest($1::text[])
            ON CONFLICT (name)
            DO NOTHING
            "#,
            &topics
        )
        .execute(&self.pool)
        .await?;

        // Insert new service and topic relation if they don't exist
        sqlx::query!(
            r#"
            INSERT INTO service_topics (service_id, topic_id)
            SELECT $1, id FROM (
                SELECT id FROM topics WHERE name = ANY($2::text[])
            ) AS topic_ids
            ON CONFLICT (service_id, topic_id)
            DO NOTHING
            "#,
            service_id,
            &topics
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }
}
