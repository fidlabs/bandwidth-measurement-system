use std::sync::Arc;

use axum::{
    debug_handler,
    extract::{Json, State},
};
use axum_extra::extract::WithRejection;
use serde::{Deserialize, Serialize};
use tracing::error;
use utoipa::ToSchema;

use crate::{
    api::api_response::*,
    service_repository::{ProviderType, Service},
    state::AppState,
};

#[derive(Deserialize, ToSchema)]
pub struct CreateServiceInput {
    #[schema(example = "my-service")]
    pub service_name: String,
    #[schema(example = "AWSFargate")]
    pub provider_type: ProviderType,
    #[schema(example = r#"["topic1", "topic2"]"#)]
    pub topics: Vec<String>,
    #[schema(example = "a-cluster", required = false)]
    pub cluster: Option<String>,
    #[schema(example = "us-east-1", required = false)]
    pub region: Option<String>,
}

#[derive(Serialize, ToSchema)]
pub struct CreateServiceResponse(pub Service);

/// Create a new service and its topics
#[utoipa::path(
    post,
    path = "/services",
    request_body(content = CreateServiceInput),
    description = r#"
**Create a new service and its topics.**

The service is a deployment unit of worker that can be scaled up or down.
"#,
    security(
        ("bearer_auth" = [])
    ),
    responses(
        (status = 200, description = "Created Service", body = CreateServiceResponse),
        (status = 400, description = "Bad Request", body = ErrorResponse),
        (status = 500, description = "Internal Server Error", body = ErrorResponse),
    ),
    tags = ["Services"],
)]
#[debug_handler]
pub async fn handle_create_service(
    State(state): State<Arc<AppState>>,
    WithRejection(Json(payload), _): WithRejection<
        Json<CreateServiceInput>,
        ApiResponse<ErrorResponse>,
    >,
) -> Result<ApiResponse<CreateServiceResponse>, ApiResponse<()>> {
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
        .repo
        .service
        .create_service(&payload.service_name, payload.provider_type, &details)
        .await
        .inspect_err(|e| {
            error!("ServiceRepository create service error: {:?}", e);
        })
        .map_err(|_| internal_server_error("Failed to create service"))?;

    // Create the service topics
    state
        .repo
        .topic
        .upsert_service_topics(&service.id, &payload.topics)
        .await
        .inspect_err(|e| {
            error!("ServiceRepository create service topics error: {:?}", e);
        })
        .map_err(|_| internal_server_error("Failed to create service topics"))?;

    Ok(ok_response(CreateServiceResponse(service)))
}
