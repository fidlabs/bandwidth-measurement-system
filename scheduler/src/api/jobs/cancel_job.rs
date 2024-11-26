use std::sync::Arc;

use axum::{
    debug_handler,
    extract::{Path, State},
};
use axum_extra::extract::WithRejection;
use serde::{Deserialize, Serialize};
use tracing::{error, info};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;
use common::api_response::*;

use crate::{
    job_repository::{JobStatus, JobWithSubJobs},
    state::AppState,
    sub_job_repository::SubJobStatus,
};

#[derive(Deserialize, ToSchema, IntoParams)]
pub struct CancelJobPathParams {
    job_id: Uuid,
}

#[derive(Serialize, ToSchema)]
pub struct CancelJobResponse(pub JobWithSubJobs);

/// Cancel a job and all its sub jobs
#[utoipa::path(
    delete,
    path = "/jobs/{job_id}",
    params (CancelJobPathParams),
    description = r#"
**Cancel a job and all its sub job.s**
"#,
    responses(
        (status = 200, description = "Job Canceled", body = CancelJobResponse),
        (status = 400, description = "Bad Request", body = ErrorResponse),
        (status = 500, description = "Internal Server Error", body = ErrorResponse),
    ),
    tags = ["Jobs"],
)]
#[debug_handler]
pub async fn handle_cancel_job(
    WithRejection(Path(params), _): WithRejection<
        Path<CancelJobPathParams>,
        ApiResponse<ErrorResponse>,
    >,
    State(state): State<Arc<AppState>>,
) -> Result<ApiResponse<CancelJobResponse>, ApiResponse<()>> {
    let job_id = params.job_id;

    info!("Getting data for job_id: {}", job_id);

    state
        .repo
        .job
        .update_job_status(&job_id, JobStatus::Canceled)
        .await
        .map_err(|e| {
            error!("Failed to cancel job: {:?}", e);
            bad_request("Failed to cancel job")
        })?;

    state
        .repo
        .sub_job
        .update_sub_jobs_status_by_job_id(&job_id, SubJobStatus::Canceled)
        .await
        .map_err(|e| {
            error!("Failed to cancel sub jobs: {:?}", e);
            bad_request("Failed to cancel sub jobs")
        })?;

    let job = state
        .repo
        .job
        .get_job_by_id_with_subjobs(&job_id)
        .await
        .map_err(|e| {
            error!("Failed to get job: {:?}", e);
            bad_request("Failed to get job")
        })?;

    Ok(ok_response(CancelJobResponse(job)))
}
