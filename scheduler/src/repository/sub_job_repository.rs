use chrono::{DateTime, Utc};
use color_eyre::Result;
use rabbitmq::JobParams;
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

    /// Extracts job parameters for constructing a JobMessage.
    /// Pulls data from this sub-job and its nested Job/JobDetails.
    pub fn job_params(&self) -> JobParams<'_> {
        JobParams {
            job_id: self.job_id,
            sub_job_id: self.id,
            url: &self.job.url,
            start_range: self.job.details.start_range,
            end_range: self.job.details.end_range,
            log_interval_ms: self.job.details.log_interval_ms,
        }
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

    /// Builder method to add partial to existing SubJobDetails.
    /// Enables chaining: `SubJobDetails::topic(location).with_partial(100)`
    pub fn with_partial(mut self, partial: i64) -> Self {
        self.partial = Some(partial);
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
            // SubJobDetails contains only primitive types (Option<i64>, Option<String>), so
            // serialization cannot fail. We use expect() to document this invariant while
            // still catching any future changes that might break this assumption.
            serde_json::to_value(details).expect("SubJobDetails serialization cannot fail"),
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(sub_job)
    }

    /// Batch insert multiple sub-jobs atomically.
    /// All sub-jobs are inserted in a single query - either all succeed or none.
    pub async fn create_sub_jobs_batch(
        &self,
        sub_jobs: Vec<(Uuid, Uuid, SubJobStatus, SubJobType, SubJobDetails)>,
    ) -> Result<(), sqlx::Error> {
        if sub_jobs.is_empty() {
            return Ok(());
        }

        let ids: Vec<Uuid> = sub_jobs.iter().map(|(id, _, _, _, _)| *id).collect();
        let job_ids: Vec<Uuid> = sub_jobs
            .iter()
            .map(|(_, job_id, _, _, _)| *job_id)
            .collect();
        let statuses: Vec<SubJobStatus> =
            sub_jobs.iter().map(|(_, _, s, _, _)| s.clone()).collect();
        let types: Vec<SubJobType> = sub_jobs.iter().map(|(_, _, _, t, _)| t.clone()).collect();
        let details: Vec<serde_json::Value> = sub_jobs
            .iter()
            .map(|(_, _, _, _, d)| {
                serde_json::to_value(d).expect("SubJobDetails serialization cannot fail")
            })
            .collect();

        sqlx::query!(
            r#"
            INSERT INTO sub_jobs (id, job_id, status, type, details)
            SELECT * FROM UNNEST(
                $1::uuid[],
                $2::uuid[],
                $3::sub_job_status[],
                $4::sub_job_type[],
                $5::jsonb[]
            )
            "#,
            &ids,
            &job_ids,
            &statuses as &[SubJobStatus],
            &types as &[SubJobType],
            &details,
        )
        .execute(&self.pool)
        .await?;

        Ok(())
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
            ORDER BY
                sub_jobs.created_at ASC,
                CASE WHEN sub_jobs.type = 'Scaling' THEN 0 ELSE 1 END ASC,
                sub_jobs.id ASC
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

    pub async fn has_pending_scaling_for_job(&self, job_id: Uuid) -> Result<bool, sqlx::Error> {
        let count = sqlx::query_scalar!(
            r#"
            SELECT COUNT(*) as "count!"
            FROM sub_jobs
            WHERE job_id = $1
              AND type = $2
              AND (status = $3 OR status = $4 OR status = $5)
            "#,
            job_id,
            SubJobType::Scaling as SubJobType,
            SubJobStatus::Created as SubJobStatus,
            SubJobStatus::Pending as SubJobStatus,
            SubJobStatus::Processing as SubJobStatus
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(count > 0)
    }

    pub async fn get_all_pending_scaling_for_job(
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
              AND (sub_jobs.status = $3 OR sub_jobs.status = $4 OR sub_jobs.status = $5)
            ORDER BY sub_jobs.created_at ASC
            "#,
            job_id,
            SubJobType::Scaling as SubJobType,
            SubJobStatus::Created as SubJobStatus,
            SubJobStatus::Pending as SubJobStatus,
            SubJobStatus::Processing as SubJobStatus
        )
        .fetch_all(&self.pool)
        .await
    }

    pub async fn get_failed_scaling_for_job(
        &self,
        job_id: Uuid,
    ) -> Result<Vec<SubJob>, sqlx::Error> {
        sqlx::query_as!(
            SubJob,
            r#"
            SELECT
                id,
                job_id,
                status as "status: SubJobStatus",
                type as "type: SubJobType",
                details,
                deadline_at
            FROM sub_jobs
            WHERE job_id = $1
              AND type = $2
              AND status = $3
            "#,
            job_id,
            SubJobType::Scaling as SubJobType,
            SubJobStatus::Failed as SubJobStatus
        )
        .fetch_all(&self.pool)
        .await
    }

    pub async fn fail_benchmark_by_job_and_topic(
        &self,
        job_id: Uuid,
        topic: &str,
        error_message: String,
    ) -> Result<bool, sqlx::Error> {
        let result = sqlx::query!(
            r#"
            UPDATE sub_jobs
            SET
                status = $1,
                details = jsonb_set(details, '{error}', $5, true)
            WHERE job_id = $2
              AND type = $3
              AND status = $4
              AND details->>'topic' = $6
            "#,
            SubJobStatus::Failed as SubJobStatus,
            job_id,
            SubJobType::CombinedDHP as SubJobType,
            SubJobStatus::Created as SubJobStatus,
            serde_json::Value::String(error_message),
            topic,
        )
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }
}
