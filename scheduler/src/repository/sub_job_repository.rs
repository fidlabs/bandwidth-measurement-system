use chrono::{DateTime, Utc};
use color_eyre::Result;
use serde::{Deserialize, Serialize};
use sqlx::{
    prelude::{FromRow, Type},
    PgPool,
};
use utoipa::ToSchema;
use uuid::Uuid;

#[derive(Deserialize, Serialize, Debug, Type, ToSchema, Clone)]
#[sqlx(type_name = "sub_job_status")]
pub enum SubJobStatus {
    Created,
    Pending,
    Processing,
    Completed,
    Failed,
    Canceled,
}

#[derive(Deserialize, Serialize, Debug, Type, ToSchema, Clone, PartialEq)]
#[sqlx(type_name = "sub_job_type")]
pub enum SubJobType {
    CombinedDHP,
    Scaling,
}

#[derive(Clone)]
pub struct SubJobRepository {
    pool: PgPool,
}

#[derive(Serialize, Deserialize, FromRow, Debug, Type, ToSchema, Clone)]
#[allow(dead_code)]
pub struct SubJob {
    pub id: Uuid,
    pub job_id: Uuid,
    pub status: SubJobStatus,
    pub r#type: SubJobType,
    pub details: serde_json::Value,
    pub deadline_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize, Deserialize, Type)]
pub struct WorkerData {
    id: Uuid,
    worker_name: String,
    is_success: Option<bool>,
    download: serde_json::Value,
    ping: serde_json::Value,
    head: serde_json::Value,
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

    pub async fn update_sub_jobs_status_by_job_id(
        &self,
        job_id: &Uuid,
        status: SubJobStatus,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"
            UPDATE sub_jobs
            SET status = $1
            WHERE job_id = $2
            "#,
            status as SubJobStatus,
            job_id,
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

    pub async fn count_pending_sub_jobs(
        &self,
        sub_job_type: SubJobType,
        job_id: &Uuid,
    ) -> Result<i64, sqlx::Error> {
        let count = sqlx::query!(
            r#"
            SELECT COUNT(*) as count
            FROM sub_jobs
            WHERE job_id = $1 AND type = $2 AND status IN ('Created', 'Pending', 'Processing')
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
            WHERE status = 'Created' OR status = 'Pending' OR status = 'Processing' 
            ORDER BY created_at ASC
            LIMIT 1
            "#,
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(sub_job)
    }

    pub async fn get_sub_job_by_id_and_type(
        &self,
        job_id: &Uuid,
        sub_job_type: SubJobType,
    ) -> Result<SubJob, sqlx::Error> {
        let sub_job = sqlx::query_as!(
            SubJob,
            r#"
            SELECT id, job_id, status as "status!: SubJobStatus", type as "type!: SubJobType", details, deadline_at
            FROM sub_jobs
            WHERE job_id = $1 AND type = $2
            "#,
            job_id,
            sub_job_type as SubJobType,
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(sub_job)
    }

    pub async fn update_sub_job_workers_count(
        &self,
        sub_job_id: &Uuid,
        workers_count: i64,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"
            UPDATE sub_jobs
            SET details = details || jsonb_build_object('workers_count', $2::bigint)
            WHERE id = $1
            "#,
            sub_job_id,
            workers_count,
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }
}
