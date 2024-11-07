use std::sync::Arc;

use crate::{
    api::api_response::{bad_request, ApiResponse, ErrorResponse},
    job_repository::JobWithSubJobsWithData,
    state::AppState,
};
use axum::{
    debug_handler,
    extract::{Path, State},
};
use axum_extra::extract::WithRejection;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tracing::{debug, error, info};
use uuid::Uuid;

use crate::api::api_response::*;

#[derive(Deserialize)]
pub struct GetJobPathParams {
    job_id: Uuid,
}

#[derive(Serialize)]
pub struct GetJobResponse {
    #[serde(flatten)]
    job: JobWithSubJobsWithData,
    summary: JobSummary,
}

#[derive(Serialize)]
pub struct JobSummary {
    pub max_download_speed: Option<f64>,
    pub average_end_latency: Option<f64>,
    pub average_gateway_latency: Option<f64>,
}

/// Get the job with sub jobs and worker data
#[debug_handler]
pub async fn handle(
    WithRejection(Path(params), _): WithRejection<
        Path<GetJobPathParams>,
        ApiResponse<ErrorResponse>,
    >,
    State(state): State<Arc<AppState>>,
) -> Result<ApiResponse<GetJobResponse>, ApiResponse<()>> {
    let job_id = params.job_id;

    info!("Getting data for job_id: {}", job_id);

    let job = state
        .repo
        .job
        .get_job_by_id_with_subjobs_and_data(job_id)
        .await
        .map_err(|e| match e {
            sqlx::Error::RowNotFound => not_found("Job data not found"),
            _ => {
                error!("Failed to get data from the database: {:?}", e);
                bad_request("Failed to get data from the database")
            }
        })?;

    debug!("Job data found for job_id: {} {:?}", job_id, job);

    let download_speeds = job.sub_jobs.iter().map(|sub_job| {
        let sub_job_download_speed = sub_job.worker_data.iter().map(|wd| {
            return wd
                .download
                .get("download_speed")
                .unwrap_or(&json!(0.0))
                .as_f64()
                .unwrap_or(0.0);
        });

        sub_job_download_speed.sum::<f64>()
    });

    let max_download_speed = download_speeds
        .max_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
        .unwrap_or(0.0);

    Ok(ok_response(GetJobResponse {
        job,
        summary: JobSummary {
            max_download_speed: Some(max_download_speed),
            average_end_latency: None,
            average_gateway_latency: None,
        },
    }))
}
