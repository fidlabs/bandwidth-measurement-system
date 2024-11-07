use std::sync::Arc;

use crate::{
    api::api_response::{bad_request, ApiResponse, ErrorResponse},
    job_repository::JobWithSubJobs,
    state::AppState,
};
use axum::{
    debug_handler,
    extract::{Query, State},
};
use axum_extra::extract::WithRejection;
use serde::{Deserialize, Serialize};
use tracing::error;

use crate::api::api_response::*;

#[derive(Deserialize)]
pub struct GetJobsPathParams {
    page: Option<u32>,
    limit: Option<u32>,
}

#[derive(Serialize)]
pub struct GetJobsResponse(pub Vec<JobWithSubJobs>);

/// Get paginated jobs with sub jobs
#[debug_handler]
pub async fn handle(
    WithRejection(Query(params), _): WithRejection<
        Query<GetJobsPathParams>,
        ApiResponse<ErrorResponse>,
    >,
    State(state): State<Arc<AppState>>,
) -> Result<ApiResponse<GetJobsResponse>, ApiResponse<()>> {
    let jobs = state
        .repo
        .job
        .get_jobs_with_subjobs(params.page, params.limit)
        .await
        .map_err(|e| match e {
            sqlx::Error::RowNotFound => not_found("Job not found"),
            _ => {
                error!("Failed to get job from the database: {:?}", e);
                bad_request("Failed to get job from the database")
            }
        })?;

    Ok(ok_response(GetJobsResponse(jobs)))
}
