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
    job_repository::{JobDetails, JobStatus, JobType},
    state::AppState,
    sub_job_repository::{SubJobDetails, SubJobStatus, SubJobType},
    url_validator::validate_and_get_file_range,
};

#[derive(Deserialize, ToSchema, Debug)]
pub struct CreateGeolocationJobInput {
    #[schema(example = "http://provider.com/file")]
    pub url: String,
    pub entity: Option<String>,
    pub note: Option<String>,
    #[schema(minimum = 10, maximum = 1024)]
    pub size_mb: Option<i64>,
    #[schema(minimum = 100, maximum = 1000)]
    pub log_interval_ms: Option<i64>,
}

#[derive(Serialize, ToSchema)]
pub struct CreateGeolocationJobResponse {
    pub job_id: Uuid,
    pub status: String,
}

/// Creates a geolocation job to find closest worker location
#[utoipa::path(
    post,
    path = "/jobs/geolocation",
    request_body(content = CreateGeolocationJobInput),
    description = "Creates a geolocation job that tests from all locations and identifies the closest one based on TTFB",
    responses(
        (status = 200, description = "Geolocation Job Created", body = CreateGeolocationJobResponse),
        (status = 400, description = "Bad Request", body = ErrorResponse),
        (status = 500, description = "Internal Server Error", body = ErrorResponse),
    ),
    tags = ["Jobs"],
)]
#[debug_handler]
pub async fn handle_create_geolocation_job(
    State(state): State<Arc<AppState>>,
    WithRejection(Json(payload), _): WithRejection<
        Json<CreateGeolocationJobInput>,
        ApiResponse<ErrorResponse>,
    >,
) -> Result<ApiResponse<CreateGeolocationJobResponse>, ApiResponse<()>> {
    info!("Creating geolocation job with payload: {:?}", payload);

    let size_mb = payload.size_mb.unwrap_or(100).clamp(10, 1024);
    let log_interval_ms = payload.log_interval_ms.unwrap_or(1000).clamp(100, 1000);

    // Get distinct locations from services
    let locations = state
        .repo
        .service
        .get_distinct_locations()
        .await
        .map_err(|_| internal_server_error("Failed to query locations"))?;

    if locations.is_empty() {
        return Err(bad_request("No locations configured"));
    }

    debug!("Found {} locations: {:?}", locations.len(), locations);

    // Validate URL and get file range (with SSRF protection)
    let (validated_url, start_range, end_range) =
        validate_and_get_file_range(&state.acl_client, &payload.url, size_mb)
            .await
            .map_err(|e| bad_request(e.to_string()))?;

    // Create job
    let job_id = Uuid::new_v4();
    let job = state
        .repo
        .job
        .create_job(
            job_id,
            validated_url.as_str().to_string(),
            &"all".to_string(), // Routing key for reference
            JobStatus::Pending,
            JobType::Geolocation,
            JobDetails::new(
                start_range,
                end_range,
                1, // 1 worker per location
                payload.entity.clone(),
                payload.note.clone(),
                log_interval_ms,
                size_mb,
            ),
        )
        .await
        .map_err(|_| internal_server_error("Failed to create job"))?;

    debug!("Geolocation job created: {:?}", job);

    // Create scaling sub-job (topic="all")
    let scaling_sub_job = state
        .repo
        .sub_job
        .create_sub_job(
            Uuid::new_v4(),
            job.id,
            SubJobStatus::Created,
            SubJobType::Scaling,
            SubJobDetails::topic("all".to_string()),
        )
        .await
        .map_err(|_| internal_server_error("Failed to create scaling sub job"))?;

    debug!("Scaling sub-job created: {:?}", scaling_sub_job);

    // Create benchmark sub-jobs (one per location)
    for location in &locations {
        let benchmark_sub_job = state
            .repo
            .sub_job
            .create_sub_job(
                Uuid::new_v4(),
                job.id,
                SubJobStatus::Created,
                SubJobType::CombinedDHP,
                SubJobDetails::topic(location.clone()),
            )
            .await
            .map_err(|_| internal_server_error("Failed to create benchmark sub job"))?;

        debug!(
            "Benchmark sub-job created for {}: {:?}",
            location, benchmark_sub_job
        );
    }

    Ok(ok_response(CreateGeolocationJobResponse {
        job_id: job.id,
        status: "pending".to_string(),
    }))
}
