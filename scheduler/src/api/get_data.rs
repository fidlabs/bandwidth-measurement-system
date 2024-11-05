use std::sync::Arc;

use crate::{
    api::api_response::{bad_request, ApiResponse, ErrorResponse},
    repository::job_repository::JobWithData,
    state::AppState,
};
use axum::{
    debug_handler,
    extract::{Query, State},
};
use axum_extra::extract::WithRejection;
use serde::{Deserialize, Serialize};
use tracing::{debug, error, info};
use uuid::Uuid;

use super::api_response::*;

#[derive(Deserialize)]
pub struct GetDataQuery {
    job_id: String,
}

#[derive(Serialize)]
pub struct GetDataResponse(pub JobWithData);

/// GET /data?job_id={job_id}
/// Get the data for a job
#[debug_handler]
pub async fn handle(
    WithRejection(Query(params), _): WithRejection<Query<GetDataQuery>, ApiResponse<ErrorResponse>>,
    State(state): State<Arc<AppState>>,
) -> Result<ApiResponse<GetDataResponse>, ApiResponse<()>> {
    // Validate the job_id
    let job_id = Uuid::parse_str(&params.job_id)
        .map_err(|_| bad_request("Invalid job_id; must be a valid UUID"))?;

    info!("Getting data for job_id: {}", job_id);

    let job = state
        .repo
        .job
        .get_job_by_id_with_data(job_id)
        .await
        .map_err(|e| match e {
            sqlx::Error::RowNotFound => not_found("Job data not found"),
            _ => {
                error!("Failed to get data from the database: {:?}", e);
                bad_request("Failed to get data from the database")
            }
        })?;

    debug!("Job data found for job_id: {} {:?}", job_id, job);

    Ok(ok_response(GetDataResponse(job)))
}
