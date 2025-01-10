use std::sync::Arc;

use axum::{
    debug_handler,
    extract::{Path, Query, State},
};
use axum_extra::extract::WithRejection;
use common::api_response::*;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tracing::{debug, error, info};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::{
    job_repository::JobWithSubJobsWithData, state::AppState, sub_job_repository::SubJobType,
};

#[derive(Deserialize, ToSchema, IntoParams)]
pub struct GetJobPathParams {
    job_id: Uuid,
}

#[derive(Serialize, Deserialize, ToSchema)]
pub struct GetJobQueryParams {
    #[schema(example = "false")]
    pub extended: Option<bool>,
}

#[derive(Serialize, ToSchema)]
pub struct GetJobResponse {
    summary: JobSummary,
    #[serde(flatten)]
    job: JobWithSubJobsWithData,
}

#[derive(Serialize, ToSchema)]
pub struct DownloadSpeed {
    sub_job_id: Uuid,
    download_speed: f64,
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
    params (GetJobPathParams),
    description = r#"
**Get the job with sub jobs and worker data.**
"#,
    responses(
        (status = 200, description = "Job Data", body = GetJobResponse),
        (status = 400, description = "Bad Request", body = ErrorResponse),
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

    let download_speeds_iter = job
        .sub_jobs
        .iter()
        .filter(|sub_job| sub_job.r#type == SubJobType::CombinedDHP)
        .map(|sub_job| {
            let sub_job_download_speed = sub_job.worker_data.iter().map(|wd| {
                wd.download
                    .get("download_speed")
                    .unwrap_or(&json!(0.0))
                    .as_f64()
                    .unwrap_or(0.0)
            });

            DownloadSpeed {
                sub_job_id: sub_job.id,
                download_speed: sub_job_download_speed.sum::<f64>(),
            }
        });

    let download_speeds: Vec<DownloadSpeed> = download_speeds_iter.collect();

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
    }))
}
