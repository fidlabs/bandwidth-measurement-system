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
use utoipa::{IntoParams, ToSchema};

use crate::api::api_response::*;

#[derive(Deserialize, ToSchema, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct GetJobsQueryParams {
    #[schema(example = 0)]
    page: Option<i64>,
    #[schema(example = 100)]
    limit: Option<i64>,
}

#[derive(Serialize, ToSchema)]
pub struct GetJobsResponse(pub Vec<JobWithSubJobs>);

/// Get paginated jobs with sub jobs
#[utoipa::path(
    get,
    path = "/jobs",
    params (GetJobsQueryParams),
    description = r#"
**Get paginated jobs with sub jobs.**
"#,
    responses(
        (status = 200, description = "Jobs Data", body = GetJobsResponse),
        (status = 400, description = "Bad Request", body = ErrorResponse),
        (status = 500, description = "Internal Server Error", body = ErrorResponse),
    ),
    tags = ["Jobs"],
)]
#[debug_handler]
pub async fn handle_get_jobs(
    WithRejection(Query(params), _): WithRejection<
        Query<GetJobsQueryParams>,
        ApiResponse<ErrorResponse>,
    >,
    State(state): State<Arc<AppState>>,
) -> Result<ApiResponse<GetJobsResponse>, ApiResponse<()>> {
    let jobs = state
        .repo
        .job
        .get_jobs_with_subjobs(
            params.page.unwrap_or(0),
            params.limit.unwrap_or(100).max(100),
        )
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
