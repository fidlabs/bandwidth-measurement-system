use axum::{
    debug_handler,
    extract::{Json, State},
};
use axum_extra::extract::WithRejection;
use color_eyre::Result;
use common::api_response::*;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{debug, info};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    job_repository::{Job, JobDetails, JobStatus, JobType},
    state::AppState,
    sub_job_repository::{SubJob, SubJobDetails, SubJobStatus, SubJobType},
    url_validator::{
        validate_and_get_file_range, validate_and_get_file_range_allowing_private_addresses,
    },
};

#[derive(Deserialize, ToSchema, Debug)]
pub struct CreateJobInput {
    #[schema(
        example = "http://yablufc.ddns.net:7878/piece/baga6ea4seaqb4lqf6fzjomlnhn3jahwxg52ewgcbjelzyflqjjuc7by224hbwla"
    )]
    pub url: String,
    #[schema(example = "us_east")]
    pub routing_key: String,
    #[schema(minimum = 1, maximum = 40)]
    pub worker_count: Option<i64>,
    pub entity: Option<String>,
    pub note: Option<String>,
    #[schema(minimum = 10, maximum = 1024)]
    pub size_mb: Option<i64>,
    #[schema(minimum = 100, maximum = 1000)]
    pub log_interval_ms: Option<i64>,
}

#[derive(Serialize, ToSchema)]
pub struct CreateJobResponse {
    #[serde(flatten)]
    pub job: Job,
    pub sub_jobs: Vec<SubJob>,
}

#[derive(Debug)]
struct CreateJobParams {
    pub url: String,
    pub routing_key: String,
    pub worker_count: i64,
    pub entity: Option<String>,
    pub note: Option<String>,
    pub size_mb: i64,
    pub log_interval_ms: i64,
}

impl TryFrom<CreateJobInput> for CreateJobParams {
    type Error = ApiResponse<()>;

    fn try_from(input: CreateJobInput) -> Result<Self, Self::Error> {
        if input.routing_key.is_empty() {
            return Err(bad_request("Routing key cannot be empty"));
        }

        Ok(CreateJobParams {
            url: input.url,
            routing_key: input.routing_key,
            worker_count: input.worker_count.unwrap_or(10).clamp(1, 40),
            entity: input.entity,
            note: input.note,
            size_mb: input.size_mb.unwrap_or(100).clamp(10, 1024), // Default 100 MB, Possible size 10-1024 MB
            log_interval_ms: input.log_interval_ms.unwrap_or(1000).clamp(100, 1000), // Default 1000 ms, Possible range 100-1000 ms
        })
    }
}

/// Creates a new Job to be processed by the worker
#[utoipa::path(
    post,
    path = "/jobs",
    request_body(content = CreateJobInput),
    description = r#"
**Creates a new Job to be processed by the worker.**

The Job consists of three subjobs:
- **Scaling SubJob**: Facilitates automatic scaling of the workers.
- **Benchmark SubJob 1**: Performs the first part of the benchmark work.
- **Benchmark SubJob 2**: Performs the second part of the benchmark work.

**All subjobs are carried out sequentially.**
    "#,
    responses(
        (status = 200, description = "Job Created", body = CreateJobResponse),
        (status = 400, description = "Bad Request", body = ErrorResponse),
        (status = 500, description = "Internal Server Error", body = ErrorResponse),
    ),
    tags = ["Jobs"],
)]
#[debug_handler]
pub async fn handle_create_job(
    State(state): State<Arc<AppState>>,
    WithRejection(Json(payload), _): WithRejection<
        Json<CreateJobInput>,
        ApiResponse<ErrorResponse>,
    >,
) -> Result<ApiResponse<CreateJobResponse>, ApiResponse<()>> {
    info!("Creating job with payload: {:?}", payload);

    // Validation
    let params: CreateJobParams = payload.try_into()?;
    let target_worker_count = params.worker_count;

    // Validate URL and get file range (with SSRF protection)
    let (validated_url, start_range, end_range) = if state.allow_private_url_validation {
        validate_and_get_file_range_allowing_private_addresses(
            &state.acl_client,
            &params.url,
            params.size_mb,
        )
        .await
    } else {
        validate_and_get_file_range(&state.acl_client, &params.url, params.size_mb).await
    }
    .map_err(|e| bad_request(e.to_string()))?;

    let job_id = Uuid::new_v4();

    let job = state
        .repo
        .job
        .create_job(
            job_id,
            validated_url.as_str().to_string(),
            &params.routing_key,
            JobStatus::Pending,
            JobType::BandwidthSaturation,
            JobDetails::new(
                start_range,
                end_range,
                target_worker_count,
                params.entity.clone(),
                params.note.clone(),
                params.log_interval_ms,
                params.size_mb,
            ),
        )
        .await
        .map_err(|e| {
            tracing::error!("Failed to create job: {:?}", e);
            internal_server_error("Failed to create job")
        })?;

    debug!("Job created successfully: {:?}", job);

    let scaling_sub_job = state
        .repo
        .sub_job
        .create_sub_job(
            Uuid::new_v4(),
            job.id,
            SubJobStatus::Created,
            SubJobType::Scaling,
            SubJobDetails::topic(job.routing_key.clone()),
        )
        .await
        .map_err(|_| internal_server_error("Failed to create scaling sub job"))?;

    let working_sub_jobs = create_working_sub_jobs(&state, &job, target_worker_count).await?;

    let sub_jobs: Vec<SubJob> = std::iter::once(scaling_sub_job)
        .chain(working_sub_jobs)
        .collect();

    debug!(
        "Job with sub jobs created successfully: {}, sub_jobs: {:?}",
        job_id, sub_jobs
    );

    Ok(ok_response(CreateJobResponse { job, sub_jobs }))
}

/// Dynamically create working sub jobs based on the worker count
async fn create_working_sub_jobs(
    state: &Arc<AppState>,
    job: &Job,
    worker_count: i64,
) -> Result<Vec<SubJob>, ApiResponse<()>> {
    let routing_key = job.routing_key.clone();

    // Each CombinedDHP sub-job includes the topic for consistent routing.
    // This allows the publish logic to use sub_job.effective_routing_key() uniformly.
    let sub_job_details: Vec<SubJobDetails> = match worker_count {
        1 => vec![SubJobDetails::partial(100).with_topic(routing_key.clone())],
        2 => vec![
            SubJobDetails::partial(50).with_topic(routing_key.clone()),
            SubJobDetails::partial(100).with_topic(routing_key.clone()),
        ],
        _ => vec![
            SubJobDetails::partial(1).with_topic(routing_key.clone()),
            SubJobDetails::partial(80).with_topic(routing_key.clone()),
            SubJobDetails::partial(100).with_topic(routing_key.clone()),
        ],
    };

    let mut sub_jobs = Vec::new();
    for details in sub_job_details {
        sub_jobs.push(create_sub_job(state, job, details).await?);
    }

    Ok(sub_jobs)
}

async fn create_sub_job(
    state: &Arc<AppState>,
    job: &Job,
    details: SubJobDetails,
) -> Result<SubJob, ApiResponse<()>> {
    let sub_job = state
        .repo
        .sub_job
        .create_sub_job(
            Uuid::new_v4(),
            job.id,
            SubJobStatus::Created,
            SubJobType::CombinedDHP,
            details,
        )
        .await
        .map_err(|_| internal_server_error("Failed to create sub job"))?;

    debug!("Sub job created successfully: {:?}", sub_job);

    Ok(sub_job)
}
