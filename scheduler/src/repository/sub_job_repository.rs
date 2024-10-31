use chrono::{DateTime, Utc};
use color_eyre::Result;
use sqlx::{
    prelude::{FromRow, Type},
    PgPool,
};
use uuid::Uuid;

#[derive(Debug, Type)]
#[sqlx(type_name = "sub_job_status", rename_all = "lowercase")]
pub enum SubJobStatus {
    Created,
    Pending,
    Processing,
    Completed,
    Failed,
    Canceled,
}

#[derive(Debug, Type)]
#[sqlx(type_name = "sub_job_type", rename_all = "lowercase")]
pub enum SubJobType {
    CombinedDHP,
    Scaling,
}

#[derive(Clone)]
pub struct SubJobRepository {
    pool: PgPool,
}

#[derive(FromRow, Debug)]
#[allow(dead_code)]
pub struct SubJob {
    pub id: Uuid,
    pub job_id: Uuid,
    pub status: SubJobStatus,
    pub r#type: SubJobType,
    pub details: serde_json::Value,
    pub deadline_at: Option<DateTime<Utc>>,
}

impl SubJobRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn create_sub_job(
        &self,
        sub_job_id: Uuid,
        job_id: Uuid,
        status: SubJobStatus,
        job_type: SubJobType,
        details: serde_json::Value,
    ) -> Result<SubJob, sqlx::Error> {
        let sub_job = sqlx::query_as!(
          SubJob,
            r#"
            INSERT INTO sub_jobs (id, job_id, status, type, details)
            VALUES ($1, $2, $3, $4, $5)
            RETURNING id, job_id, status as "status!: SubJobStatus", type as "type!: SubJobType", details, deadline_at
            "#,
            sub_job_id,
            job_id,
            status as SubJobStatus,
            job_type as SubJobType,
            details,
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(sub_job)
    }

    pub async fn update_sub_job_status(
        &self,
        sub_job_id: &Uuid,
        status: SubJobStatus,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"
            UPDATE sub_jobs
            SET status = $1
            WHERE id = $2
            "#,
            status as SubJobStatus,
            sub_job_id,
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn update_sub_job_status_with_error(
        &self,
        sub_job_id: &Uuid,
        status: SubJobStatus,
        error_message: String,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"
            UPDATE sub_jobs
            SET status = $1, details = jsonb_set(details, '{error}', $3, true)
            WHERE id = $2
            "#,
            status as SubJobStatus,
            sub_job_id,
            serde_json::Value::String(error_message),
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn update_sub_job_status_and_deadline(
        &self,
        sub_job_id: &Uuid,
        status: SubJobStatus,
        deadline: DateTime<Utc>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"
            UPDATE sub_jobs
            SET status = $1, deadline_at = $2
            WHERE id = $3
            "#,
            status as SubJobStatus,
            deadline,
            sub_job_id,
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn count_pending_sub_jobs(&self, job_id: Uuid) -> Result<i64, sqlx::Error> {
        let sub_job_type = SubJobType::CombinedDHP;
        let count = sqlx::query!(
            r#"
            SELECT COUNT(*) as count
            FROM sub_jobs
            WHERE job_id = $1 AND type = $2 AND status = 'pending'
            "#,
            job_id,
            sub_job_type as SubJobType,
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(count.count.unwrap())
    }

    pub async fn get_first_unfinished_sub_job(&self) -> Result<SubJob, sqlx::Error> {
        let sub_job = sqlx::query_as!(
            SubJob,
            r#"
            SELECT id, job_id, status as "status!: SubJobStatus", type as "type!: SubJobType", details, deadline_at
            FROM sub_jobs
            WHERE status = 'created' OR status = 'pending' OR status = 'processing'
            ORDER BY created_at ASC
            LIMIT 1
            "#,
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(sub_job)
    }
}
