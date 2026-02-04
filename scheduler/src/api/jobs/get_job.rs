use std::sync::Arc;

use axum::{
    debug_handler,
    extract::{Path, Query, State},
};
use axum_extra::extract::WithRejection;
use common::api_response::*;
use serde::{Deserialize, Serialize};
use tracing::{debug, error, info};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::{
    job_repository::{JobType, JobWithSubJobsWithData, SubJobWithData},
    repository::geolocation_repository::{GeolocationRepository, LocationResult, LocationStatus},
    state::AppState,
    sub_job_repository::SubJobType,
};

#[derive(Deserialize, ToSchema, IntoParams)]
pub struct GetJobPathParams {
    job_id: Uuid,
}

#[derive(Serialize, Deserialize, ToSchema, IntoParams)]
pub struct GetJobQueryParams {
    #[schema(default = false, example = "false")]
    pub extended: Option<bool>,
}

#[derive(Serialize, ToSchema)]
pub struct GetJobResponse {
    summary: JobSummary,
    #[serde(flatten)]
    job: JobWithSubJobsWithData,
    #[serde(skip_serializing_if = "Option::is_none")]
    closest_location: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    location_results: Option<Vec<LocationResult>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    completion: Option<CompletionInfo>,
}

#[derive(Serialize, ToSchema)]
pub struct CompletionInfo {
    pub total: i32,
    pub succeeded: i32,
    pub failed: i32,
    pub canceled: i32,
    pub is_partial: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warning: Option<String>,
}

impl CompletionInfo {
    /// Build CompletionInfo from location results, computing counts and generating
    /// appropriate warning messages for partial results.
    pub fn from_location_results(results: &[LocationResult]) -> Self {
        let succeeded = results
            .iter()
            .filter(|r| r.status == LocationStatus::Completed)
            .count() as i32;
        let failed = results
            .iter()
            .filter(|r| r.status == LocationStatus::Failed)
            .count() as i32;
        let canceled = results
            .iter()
            .filter(|r| r.status == LocationStatus::Canceled)
            .count() as i32;
        let total = results.len() as i32;
        let is_partial = failed > 0 || canceled > 0;

        let warning = match (failed > 0, canceled > 0) {
            (true, true) => Some(format!(
                "Results are partial - {} location(s) failed, {} canceled. The closest location may not be accurate.",
                failed, canceled
            )),
            (true, false) => Some(format!(
                "Results are partial - {} location(s) failed. The closest location may not be accurate.",
                failed
            )),
            (false, true) => Some(format!(
                "Results are partial - {} location(s) canceled. The closest location may not be accurate.",
                canceled
            )),
            (false, false) => None,
        };

        Self {
            total,
            succeeded,
            failed,
            canceled,
            is_partial,
            warning,
        }
    }
}

#[derive(Serialize, ToSchema)]
pub struct DownloadSpeed {
    sub_job_id: Uuid,
    download_speed: f64,
    average_time_to_first_byte_ms: f64,
}

impl SubJobWithData {
    /// Computes aggregated download metrics for this sub-job.
    /// Returns total download speed (sum across all workers) and average TTFB.
    fn to_download_speed(&self) -> DownloadSpeed {
        let mut speed_sum = 0.0;
        let mut ttfb_sum = 0.0;

        for wd in &self.worker_data {
            speed_sum += wd
                .download
                .get("download_speed")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0);

            ttfb_sum += wd
                .download
                .get("time_to_first_byte_ms")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0);
        }

        let worker_count = self.worker_data.len() as f64;
        let average_ttfb = if worker_count > 0.0 {
            ttfb_sum / worker_count
        } else {
            0.0
        };

        DownloadSpeed {
            sub_job_id: self.id,
            download_speed: speed_sum,
            average_time_to_first_byte_ms: average_ttfb,
        }
    }
}

#[derive(Serialize, ToSchema)]
pub struct JobSummary {
    pub max_download_speed: Option<f64>,
    pub download_speeds: Option<Vec<DownloadSpeed>>,
    pub average_end_latency: Option<f64>,
    pub average_gateway_latency: Option<f64>,
}

/// Get the job with sub jobs and worker data
#[utoipa::path(
    get,
    path = "/jobs/{job_id}",
    params (
        GetJobPathParams,
        GetJobQueryParams
    ),
    description = r#"
**Get the job with sub jobs and worker data.**
"#,
    responses(
        (status = 200, description = "Job Data", body = GetJobResponse),
        (status = 400, description = "Bad Request", body = ErrorResponse),
        (status = 404, description = "Job Not Found", body = ErrorResponse),
        (status = 500, description = "Internal Server Error", body = ErrorResponse),
    ),
    tags = ["Jobs"],
)]
#[debug_handler]
pub async fn handle_get_job(
    WithRejection(Path(path), _): WithRejection<Path<GetJobPathParams>, ApiResponse<ErrorResponse>>,
    Query(query): Query<GetJobQueryParams>,
    State(state): State<Arc<AppState>>,
) -> Result<ApiResponse<GetJobResponse>, ApiResponse<()>> {
    let job_id = path.job_id;
    let extended = query.extended.unwrap_or(false);

    info!("Getting data for job_id: {}", job_id);

    let job = state
        .repo
        .job
        .get_job_by_id_with_subjobs_and_data(job_id, extended)
        .await
        .map_err(|e| match e {
            sqlx::Error::RowNotFound => not_found("Job data not found"),
            _ => {
                error!("Failed to get data from the database: {:?}", e);
                bad_request("Failed to get data from the database")
            }
        })?;

    debug!("Job data found for job_id: {} {:?}", job_id, job);

    // Check if this is a geolocation job
    let (closest_location, location_results, completion) = if job.job_type == JobType::Geolocation {
        let results = state
            .repo
            .geolocation
            .get_location_results(job_id)
            .await
            .map_err(|e| {
                error!("Failed to get location results: {:?}", e);
                internal_server_error("Failed to get location results")
            })?;

        let closest = GeolocationRepository::calculate_closest_location(&results);
        let completion_info = CompletionInfo::from_location_results(&results);

        (closest, Some(results), Some(completion_info))
    } else {
        (None, None, None)
    };

    let download_speeds: Vec<DownloadSpeed> = job
        .sub_jobs
        .iter()
        .filter(|sub_job| sub_job.r#type == SubJobType::CombinedDHP)
        .map(|sub_job| sub_job.to_download_speed())
        .collect();

    let max_download_speed = download_speeds
        .iter()
        .map(|ds| ds.download_speed)
        .max_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
        .unwrap_or(0.0);

    Ok(ok_response(GetJobResponse {
        job,
        summary: JobSummary {
            max_download_speed: Some(max_download_speed),
            download_speeds: Some(download_speeds),
            average_end_latency: None,
            average_gateway_latency: None,
        },
        closest_location,
        location_results,
        completion,
    }))
}
