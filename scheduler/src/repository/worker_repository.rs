use chrono::{DateTime, Utc};
use color_eyre::Result;
use rabbitmq::WorkerStatus;
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Clone)]
pub struct WorkerRepository {
    pool: PgPool,
}

impl WorkerRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn update_worker_status(
        &self,
        worker_name: &String,
        status: &WorkerStatus,
        timestamp: DateTime<Utc>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"
            INSERT INTO workers (worker_name, status, last_seen, job_id, started_at, shutdown_at)
            VALUES ($1, $2, $3, $4, $5, $6)
            ON CONFLICT (worker_name)
            DO UPDATE SET
                status = EXCLUDED.status,
                last_seen = EXCLUDED.last_seen,
                job_id = EXCLUDED.job_id,
                started_at = CASE
                    WHEN EXCLUDED.status = 'online' THEN EXCLUDED.last_seen
                    ELSE workers.started_at
                END,
                shutdown_at = CASE
                    WHEN EXCLUDED.status = 'offline' THEN EXCLUDED.last_seen
                    ELSE workers.shutdown_at
                END
            WHERE workers.last_seen < EXCLUDED.last_seen
            "#,
            worker_name,
            status.as_str(),
            timestamp,
            None::<Uuid>,
            None::<DateTime<Utc>>,
            None::<DateTime<Utc>>
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn update_worker_job(
        &self,
        worker_name: String,
        job_id: Option<Uuid>,
        timestamp: DateTime<Utc>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"
            UPDATE workers
            SET
                last_seen = $2,
                job_id = $3
            WHERE worker_name = $1 AND workers.last_seen < $2
            "#,
            worker_name,
            timestamp,
            job_id
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn update_worker_heartbeat(
        &self,
        worker_name: String,
        timestamp: DateTime<Utc>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"
            UPDATE workers
            SET
                last_seen = $2
            WHERE worker_name = $1 AND workers.last_seen < $2
            "#,
            worker_name,
            timestamp
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }
}
