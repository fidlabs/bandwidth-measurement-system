use axum::{
    debug_handler,
    extract::{Json, State},
};
use axum_extra::extract::WithRejection;
use color_eyre::Result;
use rand::Rng;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;
use tracing::{debug, info};
use url::Url;
use uuid::Uuid;

use crate::{
    api::api_response::*,
    job_repository::{Job, JobStatus},
    state::AppState,
    sub_job_repository::{SubJob, SubJobStatus, SubJobType},
};

#[derive(Deserialize)]
pub struct JobInput {
    pub url: String,
    pub routing_key: String,
}

#[derive(Serialize)]
pub struct JobResponse {
    pub job_id: Uuid,
    pub sub_jobs: Vec<Uuid>,
}

/// Create a new job to be processed by the worker
#[debug_handler]
pub async fn handle(
    State(state): State<Arc<AppState>>,
    WithRejection(Json(payload), _): WithRejection<Json<JobInput>, ApiResponse<ErrorResponse>>,
) -> Result<ApiResponse<JobResponse>, ApiResponse<()>> {
    // Validation
    let url = validate_url(&payload)?;
    validate_routing_key(&payload)?;

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
            json!({
                "start_range": start_range,
                "end_range": end_range,
            }),
        )
        .await
        .map_err(|_| internal_server_error("Failed to create job"))?;

    debug!("Job created successfully: {:?}", job);

    state
        .repo
        .sub_job
        .create_sub_job(
            Uuid::new_v4(),
            job.id,
            SubJobStatus::Created,
            SubJobType::Scaling,
            json!({
                "topic": job.routing_key,
            }),
        )
        .await
        .map_err(|_| internal_server_error("Failed to create scaling sub job"))?;

    let sub_job_1 = create_sub_job(&state, &job).await?;
    let sub_job_2 = create_sub_job(&state, &job).await?;

    let sub_jobs = vec![sub_job_1.id, sub_job_2.id];

    info!(
        "Job with sub jobs created successfully: {}, sub_jobs: {:?}",
        job_id, sub_jobs
    );

    Ok(ok_response(JobResponse { job_id, sub_jobs }))
}

/// Validate url and its scheme
fn validate_url(payload: &JobInput) -> Result<Url, ApiResponse<()>> {
    let url = Url::parse(&payload.url).map_err(|_| bad_request("Invalid URL provided"))?;
    match url.scheme() {
        "http" | "https" => Ok(url),
        _ => Err(bad_request("URL scheme must be http or https")),
    }
}

/// Validate routing key
/// In future we want to validate if the routing key is valid, maybe by checking a set of allowed keys
fn validate_routing_key(payload: &JobInput) -> Result<(), ApiResponse<()>> {
    if payload.routing_key.is_empty() {
        return Err(bad_request("Routing key cannot be empty"));
    }

    Ok(())
}

/// Get a random range of 100MB from the file using HEAD request
async fn get_file_range_for_file(url: &str) -> Result<(u64, u64), ApiResponse<()>> {
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
        .parse::<u64>()
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

async fn create_sub_job(state: &Arc<AppState>, job: &Job) -> Result<SubJob, ApiResponse<()>> {
    let sub_job = state
        .repo
        .sub_job
        .create_sub_job(
            Uuid::new_v4(),
            job.id,
            SubJobStatus::Created,
            SubJobType::CombinedDHP,
            json!({}),
        )
        .await
        .map_err(|_| internal_server_error("Failed to create sub job"))?;

    debug!("Sub job created successfully: {:?}", sub_job);

    Ok(sub_job)
}
