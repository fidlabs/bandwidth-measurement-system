use color_eyre::Result;
use serde::{Deserialize, Serialize};
use sqlx::{
    prelude::{FromRow, Type},
    types::Json,
    PgPool,
};
use uuid::Uuid;

use super::data_repository::WorkerData;

#[derive(Debug, Type)]
#[sqlx(type_name = "job_status", rename_all = "lowercase")]
pub enum JobStatus {
    Created,
    Pending,
    Processing,
    Completed,
    Failed,
}

#[derive(Clone)]
pub struct JobRepository {
    pool: PgPool,
}

#[derive(Serialize, Debug, FromRow, Type)]
pub struct JobWithData {
    pub id: Uuid,
    pub url: Option<String>,
    pub routing_key: Option<String>,
    pub details: Option<serde_json::Value>,
    pub data: Vec<Json<WorkerData>>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct JobDetails {
    pub start_range: u64,
    pub end_range: u64,
    pub workers_count: Option<i64>,
}
impl From<serde_json::Value> for JobDetails {
    fn from(value: serde_json::Value) -> Self {
        serde_json::from_value(value).expect("Failed to convert serde_json::Value to JobDetails")
    }
}

#[derive(Debug, FromRow)]
#[allow(dead_code)]
pub struct Job {
    pub id: Uuid,
    pub url: String,
    pub routing_key: String,
    pub status: JobStatus,
    pub details: JobDetails,
}

impl JobRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn create_job(
        &self,
        job_id: Uuid,
        url: String,
        routing_key: &String,
        status: JobStatus,
        details: serde_json::Value,
    ) -> Result<Job, sqlx::Error> {
        let job = sqlx::query_as!(
            Job,
            r#"
            INSERT INTO jobs (id, url, routing_key, status, details)
            VALUES ($1, $2, $3, $4, $5)
            RETURNING id, url, routing_key, status as "status!: JobStatus", details as "details!: serde_json::Value"
            "#,
            job_id,
            url,
            routing_key,
            status as JobStatus,
            details,
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(job)
    }

    pub async fn get_job_by_id(&self, job_id: &Uuid) -> Result<Job, sqlx::Error> {
        let job = sqlx::query_as!(
            Job,
            r#"
            SELECT id, url, routing_key, status as "status!: JobStatus", details as "details!: serde_json::Value"
            FROM jobs
            WHERE id = $1
            "#,
            job_id
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(job)
    }

    pub async fn get_job_by_id_with_data(&self, job_id: Uuid) -> Result<JobWithData, sqlx::Error> {
        let job = sqlx::query_as!(
            JobWithData,
            r#"
            SELECT
                jobs.id,
                jobs.url,
                jobs.routing_key,
                jobs.details,
                COALESCE(
                    ARRAY_AGG(
                        JSON_BUILD_OBJECT(
                            'id', d.id,
                            'worker_name', d.worker_name,
                            'is_success', d.is_success,
                            'download', d.download,
                            'ping', d.ping,
                            'head', d.head
                        )
                    ) FILTER (WHERE d.id IS NOT NULL),
                    ARRAY[]::json[]
                ) AS "data!: Vec<Json<WorkerData>>"
            FROM jobs
            LEFT JOIN worker_data as d ON jobs.id = d.job_id
            WHERE jobs.id = $1
            GROUP BY jobs.id
            "#,
            job_id
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(job)
    }

    pub async fn update_job_status(
        &self,
        job_id: Uuid,
        status: JobStatus,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"
            UPDATE jobs
            SET status = $1
            WHERE id = $2
            "#,
            status as JobStatus,
            job_id,
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn update_job_workers_count(
        &self,
        job_id: &Uuid,
        expected_workers: i64,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"
            UPDATE jobs
            SET details = jsonb_set(details, '{workers_count}', $2, true)
            WHERE id = $1
            "#,
            job_id,
            serde_json::Value::Number(expected_workers.into()),
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }
}
