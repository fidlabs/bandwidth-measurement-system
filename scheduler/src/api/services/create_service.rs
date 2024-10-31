use std::sync::Arc;

use axum::{
    debug_handler,
    extract::{Json, State},
};
use axum_extra::extract::WithRejection;
use serde::Deserialize;
use tracing::error;

use crate::{
    api::api_response::*,
    service_repository::{ProviderType, Service},
    state::AppState,
};

#[derive(Deserialize)]
pub struct CreateServiceInput {
    pub service_name: String,
    pub provider_type: ProviderType,
    pub topics: Vec<String>,
    pub cluster: Option<String>,
    pub region: Option<String>,
}

/// POST /services
/// Create a new service and its topics
#[debug_handler]
pub async fn handle(
    State(state): State<Arc<AppState>>,
    WithRejection(Json(payload), _): WithRejection<
        Json<CreateServiceInput>,
        ApiResponse<ErrorResponse>,
    >,
) -> Result<ApiResponse<Service>, ApiResponse<()>> {
    // Validation
    if payload.topics.is_empty() {
        return Err(bad_request("Field 'topics' cannot be empty"))?;
    }

    // Validate and construct the details field based on the provider type
    let details = match payload.provider_type {
        ProviderType::DockerLocal => serde_json::json!({}),
        ProviderType::AWSFargate => {
            let cluster = payload
                .cluster
                .ok_or(bad_request("Field 'cluster' is required"))?;
            let region = payload
                .region
                .ok_or(bad_request("Field 'region' is required"))?;
            serde_json::json!({
                "cluster": cluster,
                "region": region,
            })
        }
    };

    // Create the service
    let service = state
        .service_repo
        .create_service(&payload.service_name, payload.provider_type, &details)
        .await
        .inspect_err(|e| {
            error!("ServiceRepository create service error: {:?}", e);
        })
        .map_err(|_| internal_server_error("Failed to create service"))?;

    // Create the service topics
    state
        .topic_repo
        .upsert_service_topics(&service.id, &payload.topics)
        .await
        .inspect_err(|e| {
            error!("ServiceRepository create service topics error: {:?}", e);
        })
        .map_err(|_| internal_server_error("Failed to create service topics"))?;

    Ok(ok_response(service))
}
