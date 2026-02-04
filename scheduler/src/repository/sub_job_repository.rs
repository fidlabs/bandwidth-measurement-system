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

use super::job_repository::Job;

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

#[derive(Serialize, Deserialize, FromRow, Debug, Type, ToSchema)]
#[allow(dead_code)]
pub struct SubJobWithJob {
    pub id: Uuid,
    pub job_id: Uuid,
    pub status: SubJobStatus,
    pub r#type: SubJobType,
    pub details: serde_json::Value,
    pub deadline_at: Option<DateTime<Utc>>,
    #[schema(value_type = Job)]
    pub job: Json<Job>,
}

impl SubJobWithJob {
    /// Returns the topic from sub-job details if present.
    pub fn topic(&self) -> Option<&str> {
        self.details.get("topic").and_then(|v| v.as_str())
    }

    /// Returns the effective routing key for this sub-job.
    /// Prefers topic from details, falls back to job's routing_key for backward compatibility
    /// with old jobs that don't have topic in CombinedDHP sub-job details.
    pub fn effective_routing_key(&self) -> String {
        self.topic()
            .map(|s| s.to_string())
            .unwrap_or_else(|| self.job.routing_key.clone())
    }
}

#[allow(dead_code)]
#[derive(Debug, Serialize, Deserialize, Type)]
pub struct WorkerData {
    id: Uuid,
    worker_name: String,
    is_success: Option<bool>,
    download: serde_json::Value,
    ping: serde_json::Value,
    head: serde_json::Value,
}

#[derive(Serialize, Default)]
pub struct SubJobDetails {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub partial: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workers_count: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub topic: Option<String>,
}
impl SubJobDetails {
    pub fn partial(partial: i64) -> Self {
        SubJobDetails {
            partial: Some(partial),
            ..Default::default()
        }
    }

    pub fn topic(topic: String) -> Self {
        SubJobDetails {
            topic: Some(topic),
            ..Default::default()
        }
    }

    /// Builder method to add topic to existing SubJobDetails.
    /// Enables chaining: `SubJobDetails::partial(100).with_topic(routing_key)`
    pub fn with_topic(mut self, topic: String) -> Self {
        self.topic = Some(topic);
        self
    }
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
        details: SubJobDetails,
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
            serde_json::to_value(details).unwrap(),
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
            WHERE job_id = $1
              AND type = $2
              AND (status = $3 OR status = $4 OR status = $5)
            "#,
            job_id,
            sub_job_type as SubJobType,
            SubJobStatus::Created as SubJobStatus,
            SubJobStatus::Pending as SubJobStatus,
            SubJobStatus::Processing as SubJobStatus,
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(count.count.unwrap())
    }

    pub async fn get_first_unfinished_sub_job(&self) -> Result<SubJobWithJob, sqlx::Error> {
        let sub_job = sqlx::query_as!(
            SubJobWithJob,
            r#"
            SELECT
                sub_jobs.id,
                sub_jobs.job_id,
                sub_jobs.status as "status: SubJobStatus",
                sub_jobs.type as "type: SubJobType",
                sub_jobs.details,
                sub_jobs.deadline_at,
                JSON_BUILD_OBJECT(
                    'id', jobs.id,
                    'url', jobs.url,
                    'routing_key', jobs.routing_key,
                    'status', jobs.status,
                    'job_type', jobs.job_type,
                    'details', jobs.details
                ) as "job!: Json<Job>"
            FROM sub_jobs
            INNER JOIN jobs ON sub_jobs.job_id = jobs.id
            WHERE sub_jobs.status = $1
               OR sub_jobs.status = $2
               OR sub_jobs.status = $3
            ORDER BY sub_jobs.created_at ASC
            LIMIT 1
            "#,
            SubJobStatus::Created as SubJobStatus,
            SubJobStatus::Pending as SubJobStatus,
            SubJobStatus::Processing as SubJobStatus,
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(sub_job)
    }

    pub async fn get_sub_job_by_id_and_type(
        &self,
        job_id: &Uuid,
        sub_job_type: SubJobType,
    ) -> Result<SubJobWithJob, sqlx::Error> {
        let sub_job = sqlx::query_as!(
            SubJobWithJob,
            r#"
            SELECT
                sub_jobs.id,
                sub_jobs.job_id,
                sub_jobs.status as "status: SubJobStatus",
                sub_jobs.type as "type: SubJobType",
                sub_jobs.details,
                sub_jobs.deadline_at,
                JSON_BUILD_OBJECT(
                    'id', jobs.id,
                    'url', jobs.url,
                    'routing_key', jobs.routing_key,
                    'status', jobs.status,
                    'job_type', jobs.job_type,
                    'details', jobs.details
                ) as "job!: Json<Job>"
            FROM
                sub_jobs
            JOIN
                jobs ON sub_jobs.job_id = jobs.id
            WHERE
                sub_jobs.job_id = $1 AND sub_jobs.type = $2
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

    pub async fn get_all_pending_benchmarks_for_job(
        &self,
        job_id: Uuid,
    ) -> Result<Vec<SubJobWithJob>, sqlx::Error> {
        sqlx::query_as!(
            SubJobWithJob,
            r#"
            SELECT
                sub_jobs.id,
                sub_jobs.job_id,
                sub_jobs.status as "status: SubJobStatus",
                sub_jobs.type as "type: SubJobType",
                sub_jobs.details,
                sub_jobs.deadline_at,
                JSON_BUILD_OBJECT(
                    'id', jobs.id,
                    'url', jobs.url,
                    'routing_key', jobs.routing_key,
                    'status', jobs.status,
                    'job_type', jobs.job_type,
                    'details', jobs.details
                ) as "job!: Json<Job>"
            FROM sub_jobs
            INNER JOIN jobs ON sub_jobs.job_id = jobs.id
            WHERE sub_jobs.job_id = $1
              AND sub_jobs.type = $2
              AND sub_jobs.status = $3
            ORDER BY sub_jobs.created_at ASC
            "#,
            job_id,
            SubJobType::CombinedDHP as SubJobType,
            SubJobStatus::Created as SubJobStatus
        )
        .fetch_all(&self.pool)
        .await
    }
}
