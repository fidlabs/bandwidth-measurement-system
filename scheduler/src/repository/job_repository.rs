use chrono::{DateTime, Utc};
use color_eyre::Result;
use serde::{Deserialize, Serialize};
use sqlx::{
    prelude::{FromRow, Type},
    types::Json,
    PgPool,
};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::sub_job_repository::{SubJob, SubJobStatus, SubJobType};

#[derive(Debug, Type, Serialize, Deserialize, ToSchema)]
#[sqlx(type_name = "job_status")]
pub enum JobStatus {
    Created,
    Pending,
    Processing,
    Completed,
    Failed,
    Canceled,
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

#[derive(Serialize, Deserialize, Debug, FromRow, ToSchema)]
pub struct JobWithSubJobsWithData {
    pub id: Uuid,
    pub url: String,
    pub routing_key: String,
    pub status: JobStatus,
    pub details: JobDetails,
    #[schema(value_type = Vec<SubJobWithData>)]
    pub sub_jobs: Json<Vec<SubJobWithData>>,
}

#[derive(Serialize, Deserialize, FromRow, Debug, Type, ToSchema, Clone)]
pub struct SubJobWithData {
    pub id: Uuid,
    pub job_id: Uuid,
    pub status: SubJobStatus,
    pub r#type: SubJobType,
    pub details: serde_json::Value,
    pub deadline_at: Option<DateTime<Utc>>,
    pub worker_data: Vec<WorkerData>,
}

#[derive(Serialize, Deserialize, FromRow, Debug, Type, ToSchema, Clone)]
pub struct WorkerData {
    pub id: Uuid,
    pub worker_name: String,
    pub is_success: Option<bool>,
    pub download: serde_json::Value,
    pub ping: serde_json::Value,
    pub head: serde_json::Value,
}

#[derive(Serialize, Deserialize, FromRow, Debug, Type)]
pub struct WorkerDataDownload {
    end_time: DateTime<Utc>,
    total_bytes: i64,
    elapsed_secs: f64,
    download_speed: f64,
    job_start_time: DateTime<Utc>,
    download_start_time: DateTime<Utc>,
    time_to_first_byte_ms: f64,
    second_by_second_logs: Option<serde_json::Value>,
}

#[derive(Serialize, Deserialize, FromRow, Debug, Type)]
pub struct WorkerDataError {
    error: String,
}

#[derive(Serialize, Deserialize, FromRow, Type, Debug, ToSchema)]
pub struct JobDetails {
    pub start_range: i64,
    pub end_range: i64,
    pub workers_count: Option<i64>,
}
impl From<serde_json::Value> for JobDetails {
    fn from(value: serde_json::Value) -> Self {
        serde_json::from_value(value).expect("Failed to convert serde_json::Value to JobDetails")
    }
}

#[derive(Debug, FromRow, Serialize, ToSchema)]
#[allow(dead_code)]
pub struct Job {
    pub id: Uuid,
    pub url: String,
    pub routing_key: String,
    pub status: JobStatus,
    pub details: JobDetails,
}

#[derive(Serialize, Deserialize, Debug, FromRow, ToSchema)]
pub struct JobWithSubJobs {
    pub id: Uuid,
    pub url: String,
    pub routing_key: String,
    pub status: JobStatus,
    pub details: JobDetails,
    #[schema(value_type = Vec<SubJob>)]
    pub sub_jobs: Json<Vec<SubJob>>,
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

    pub async fn update_job_status(
        &self,
        job_id: &Uuid,
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
            SET details = details || jsonb_build_object('workers_count', $2::bigint)
            WHERE id = $1
            "#,
            job_id,
            expected_workers,
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn get_job_by_id_with_subjobs_and_data(
        &self,
        job_id: Uuid,
    ) -> Result<JobWithSubJobsWithData, sqlx::Error> {
        let job = sqlx::query_as!(
            JobWithSubJobsWithData,
            r#"
            SELECT
                j.id,
                j.url,
                j.routing_key,
                j.status AS "status!: JobStatus",
                j.details AS "details!: serde_json::Value",
                COALESCE(sub_jobs_agg.sub_jobs, '[]'::json) AS "sub_jobs!: Json<Vec<SubJobWithData>>"
            FROM jobs j
            LEFT JOIN LATERAL (
                SELECT JSON_AGG(
                    JSON_BUILD_OBJECT(
                        'id', sj.id,
                        'job_id', sj.job_id,
                        'status', sj.status,
                        'type', sj.type,
                        'details', sj.details,
                        'deadline_at', sj.deadline_at,
                        'worker_data', COALESCE(worker_data_agg.worker_data, '[]'::json)
                    )
                    ORDER BY sj.created_at DESC
                ) AS "sub_jobs"
                FROM sub_jobs sj
                LEFT JOIN LATERAL (
                    SELECT JSON_AGG(
                        JSON_BUILD_OBJECT(
                            'id', d.id,
                            'worker_name', d.worker_name,
                            'is_success', COALESCE(d.is_success, false),
                            'download', d.download - 'second_by_second_logs',
                            'ping', d.ping,
                            'head', d.head
                        )
                        ORDER BY d.created_at DESC
                    ) AS "worker_data"
                    FROM worker_data d
                    WHERE d.sub_job_id = sj.id
                ) worker_data_agg ON TRUE
                WHERE sj.job_id = j.id
            ) sub_jobs_agg ON TRUE
            WHERE j.id = $1
            "#,
            job_id
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(job)
    }

    pub async fn get_jobs_with_subjobs(
        &self,
        page: i64,
        limit: i64,
    ) -> Result<Vec<JobWithSubJobs>, sqlx::Error> {
        let jobs = sqlx::query_as!(
            JobWithSubJobs,
            r#"
            SELECT
                j.id,
                j.url,
                j.routing_key,
                j.status AS "status!: JobStatus",
                j.details AS "details!: serde_json::Value",
                COALESCE(sub_jobs_agg.sub_jobs, '[]'::json) AS "sub_jobs!: Json<Vec<SubJob>>"
            FROM jobs j
            LEFT JOIN LATERAL (
                SELECT JSON_AGG(
                    JSON_BUILD_OBJECT(
                        'id', sj.id,
                        'job_id', sj.job_id,
                        'status', sj.status,
                        'type', sj.type,
                        'details', sj.details,
                        'deadline_at', sj.deadline_at
                    )
                    ORDER BY sj.created_at DESC
                ) AS "sub_jobs"
                FROM sub_jobs sj
                WHERE sj.job_id = j.id
            ) sub_jobs_agg ON TRUE
            ORDER BY created_at DESC
            LIMIT $1 OFFSET $2
            "#,
            limit,
            page * limit
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(jobs)
    }

    pub async fn get_job_by_id_with_subjobs(
        &self,
        job_id: &Uuid,
    ) -> Result<JobWithSubJobs, sqlx::Error> {
        let jobs = sqlx::query_as!(
            JobWithSubJobs,
            r#"
            SELECT
                j.id,
                j.url,
                j.routing_key,
                j.status AS "status!: JobStatus",
                j.details AS "details!: serde_json::Value",
                COALESCE(sub_jobs_agg.sub_jobs, '[]'::json) AS "sub_jobs!: Json<Vec<SubJob>>"
            FROM jobs j
            LEFT JOIN LATERAL (
                SELECT JSON_AGG(
                    JSON_BUILD_OBJECT(
                        'id', sj.id,
                        'job_id', sj.job_id,
                        'status', sj.status,
                        'type', sj.type,
                        'details', sj.details,
                        'deadline_at', sj.deadline_at
                    )
                ) AS "sub_jobs"
                FROM sub_jobs sj
                WHERE sj.job_id = j.id
            ) sub_jobs_agg ON TRUE
            WHERE j.id = $1
            "#,
            job_id
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(jobs)
    }
}
