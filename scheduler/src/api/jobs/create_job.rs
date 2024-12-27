use axum::{
    debug_handler,
    extract::{Json, State},
};
use axum_extra::extract::WithRejection;
use color_eyre::Result;
use rand::Rng;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{debug, info};
use url::Url;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    api::api_response::*,
    job_repository::{Job, JobDetails, JobStatus},
    state::AppState,
    sub_job_repository::{SubJob, SubJobDetails, SubJobStatus, SubJobType},
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
}

#[derive(Serialize, ToSchema)]
pub struct CreateJobResponse {
    #[serde(flatten)]
    pub job: Job,
    pub sub_jobs: Vec<SubJob>,
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
    let url = validate_url(&payload)?;
    validate_routing_key(&payload)?;

    let target_worker_count = payload.worker_count.unwrap_or(10).clamp(1, 40);

    // Create the job
    let (start_range, end_range) = get_file_range_for_file(url.as_ref()).await?;
    let job_id = Uuid::new_v4();

    let job = state
        .repo
        .job
        .create_job(
            job_id,
            url.to_string(),
            &payload.routing_key,
            JobStatus::Pending,
            JobDetails::new(
                start_range,
                end_range,
                target_worker_count,
                payload.entity.clone(),
                payload.note.clone(),
            ),
        )
        .await
        .map_err(|_| internal_server_error("Failed to create job"))?;

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

    let sub_jobs = vec![
        scaling_sub_job,
        create_sub_job(&state, &job, SubJobDetails::partial(80)).await?,
        create_sub_job(&state, &job, SubJobDetails::empty()).await?,
    ];

    debug!(
        "Job with sub jobs created successfully: {}, sub_jobs: {:?}",
        job_id, sub_jobs
    );

    Ok(ok_response(CreateJobResponse { job, sub_jobs }))
}

/// Validate url and its scheme
fn validate_url(payload: &CreateJobInput) -> Result<Url, ApiResponse<()>> {
    let url = Url::parse(&payload.url).map_err(|_| bad_request("Invalid URL provided"))?;
    match url.scheme() {
        "http" | "https" => Ok(url),
        _ => Err(bad_request("URL scheme must be http or https")),
    }
}

/// Validate routing key
/// In future we want to validate if the routing key is valid, maybe by checking a set of allowed keys
fn validate_routing_key(payload: &CreateJobInput) -> Result<(), ApiResponse<()>> {
    if payload.routing_key.is_empty() {
        return Err(bad_request("Routing key cannot be empty"));
    }

    Ok(())
}

/// Get a random range of 100MB from the file using HEAD request
async fn get_file_range_for_file(url: &str) -> Result<(i64, i64), ApiResponse<()>> {
    let response = Client::new()
        .head(url)
        .send()
        .await
        .map_err(|e| bad_request(format!("Failed to execute HEAD request {}", e)))?;

    debug!("Response: {:?}", response);

    // For some freak reason response.content_length() is returning 0
    let content_length = response
        .headers()
        .get(reqwest::header::CONTENT_LENGTH)
        .ok_or_else(|| bad_request("Content-Length header is missing in the response"))?
        .to_str()
        .map_err(|e| bad_request(format!("Failed to parse Content-Length header: {}", e)))?
        .parse::<i64>()
        .map_err(|e| bad_request(format!("Failed to parse Content-Length header: {}", e)))?;

    debug!("Content-Length: {:?}", content_length);

    let size_mb = 100; // 100 MB
    let size = size_mb * 1024 * 1024;

    if content_length < size {
        return Err(bad_request(format!(
            "File size is less than {} MB",
            size_mb
        )));
    }

    let mut rng = rand::thread_rng();
    let start_range = rng.gen_range(0..content_length - size);
    let end_range = start_range + size;

    Ok((start_range, end_range))
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
