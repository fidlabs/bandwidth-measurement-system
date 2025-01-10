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
                last_seen = $2,
                status = 'online'
            WHERE worker_name = $1 AND workers.last_seen < $2
            "#,
            worker_name,
            timestamp
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn get_workers_online_with_topic(
        &self,
        topic: &String,
    ) -> Result<Vec<String>, sqlx::Error> {
        let workers = sqlx::query!(
            r#"
            SELECT w.worker_name
            FROM workers as w
            JOIN worker_topics as wt ON w.worker_name = wt.worker_name
            WHERE w.status = 'online' AND wt.topic_id = (
                SELECT id FROM topics WHERE name = $1
            )
            "#,
            topic,
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(workers.into_iter().map(|w| w.worker_name).collect())
    }

    pub async fn get_inactive_online_workers(&self) -> Result<Vec<String>, sqlx::Error> {
        let workers = sqlx::query!(
            r#"
            SELECT worker_name
            FROM workers
            WHERE status = 'online' AND last_seen < NOW() - INTERVAL '1 minute'
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(workers.into_iter().map(|w| w.worker_name).collect())
    }

    pub async fn set_worker_offline(&self, worker_name: &String) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"
            UPDATE workers
            SET status = 'offline'
            WHERE worker_name = $1
            "#,
            worker_name
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }
}
