use axum::{
    debug_handler,
    extract::{Json, State},
};
use axum_extra::extract::WithRejection;
use color_eyre::Result;
use common::api_response::*;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, sync::Arc};
use tracing::{debug, info};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    job_repository::{JobDetails, JobStatus, JobType},
    state::AppState,
    sub_job_repository::{SubJobDetails, SubJobStatus, SubJobType},
    url_validator::{
        validate_and_get_file_range, validate_and_get_file_range_allowing_private_addresses,
    },
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

    let topic_mismatches = state
        .repo
        .service
        .get_services_with_location_topic_mismatch()
        .await
        .map_err(|_| internal_server_error("Failed to validate location topics"))?;

    let mut location_errors = HashMap::new();
    for location in &locations {
        for service in &topic_mismatches {
            if service.topics.iter().any(|topic| topic == location) {
                location_errors.entry(location.clone()).or_insert_with(|| {
                    format!(
                        "Location '{}' topic is also attached to service '{}' with location '{}'",
                        location,
                        service.name,
                        service.location.as_deref().unwrap_or("NULL")
                    )
                });
            }
        }

        let services = state
            .repo
            .service
            .get_services_enabled_by_topic(location)
            .await
            .map_err(|_| internal_server_error("Failed to validate location"))?;

        if services.is_empty() {
            location_errors.entry(location.clone()).or_insert_with(|| {
                format!(
                    "Location '{}' has no services with matching topic",
                    location
                )
            });
        }
    }

    debug!("Found {} locations: {:?}", locations.len(), locations);

    // Validate URL and get file range (with SSRF protection)
    let (validated_url, start_range, end_range) = if state.allow_private_url_validation {
        validate_and_get_file_range_allowing_private_addresses(
            &state.acl_client,
            &payload.url,
            size_mb,
        )
        .await
    } else {
        validate_and_get_file_range(&state.acl_client, &payload.url, size_mb).await
    }
    .map_err(|e| bad_request(e.to_string()))?;

    let has_invalid_service_config = !location_errors.is_empty();
    let invalid_config_error = if has_invalid_service_config {
        Some(format!(
            "Invalid geolocation service configuration: {}",
            location_errors
                .values()
                .cloned()
                .collect::<Vec<_>>()
                .join("; ")
        ))
    } else {
        None
    };

    let job_id = Uuid::new_v4();
    let job = state
        .repo
        .job
        .create_job(
            job_id,
            validated_url.as_str().to_string(),
            &"all".to_string(), // Routing key for reference
            if has_invalid_service_config {
                JobStatus::Failed
            } else {
                JobStatus::Pending
            },
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

    // Build all sub-jobs for batch insert
    let mut sub_jobs_to_create = Vec::with_capacity(locations.len() * 2);

    for location in &locations {
        let sub_job_status = if has_invalid_service_config {
            SubJobStatus::Failed
        } else {
            SubJobStatus::Created
        };
        let sub_job_error = location_errors
            .get(location)
            .cloned()
            .or_else(|| invalid_config_error.clone());

        let mut scaling_details = SubJobDetails::topic(location.clone());
        let mut benchmark_details = SubJobDetails::topic(location.clone()).with_partial(100);

        if let Some(error) = sub_job_error {
            scaling_details = scaling_details.with_error(error.clone());
            benchmark_details = benchmark_details.with_error(error);
        }

        // Scaling sub-job
        sub_jobs_to_create.push((
            Uuid::new_v4(),
            job.id,
            sub_job_status.clone(),
            SubJobType::Scaling,
            scaling_details,
        ));

        // Benchmark sub-job with explicit partial: 100
        sub_jobs_to_create.push((
            Uuid::new_v4(),
            job.id,
            sub_job_status,
            SubJobType::CombinedDHP,
            benchmark_details,
        ));
    }

    // Single atomic insert for all sub-jobs
    state
        .repo
        .sub_job
        .create_sub_jobs_batch(sub_jobs_to_create)
        .await
        .map_err(|_| internal_server_error("Failed to create sub jobs"))?;

    debug!(
        "Created {} sub-jobs for geolocation job",
        locations.len() * 2
    );

    Ok(ok_response(CreateGeolocationJobResponse {
        job_id: job.id,
        status: if has_invalid_service_config {
            "failed".to_string()
        } else {
            "pending".to_string()
        },
    }))
}
