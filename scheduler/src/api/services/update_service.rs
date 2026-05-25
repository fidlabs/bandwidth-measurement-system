use std::sync::Arc;

use axum::{
    debug_handler,
    extract::{Json, Path, State},
};
use axum_extra::extract::WithRejection;
use common::api_response::*;
use serde::{Deserialize, Serialize};
use tracing::error;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::{service_repository::Service, state::AppState};

#[derive(Deserialize, ToSchema, IntoParams)]
pub struct UpdateServicePathInput {
    pub service_id: Uuid,
}

#[derive(Deserialize, ToSchema)]
pub struct UpdateServiceInput {
    pub is_enabled: Option<bool>,
    pub location: Option<String>,
    pub topics: Option<Vec<String>>,
}

// OpenAPI schema wrapper - handler returns Service directly
#[allow(dead_code)]
#[derive(Serialize, ToSchema)]
pub struct UpdateServiceResponse(pub Service);

/// Create a new service and its topics
#[utoipa::path(
    put,
    path = "/services/{service_id}",
    params(UpdateServicePathInput),
    request_body(content = UpdateServiceInput),
    security(
        ("bearer_auth" = [])
    ),
    responses(
        (status = 200, description = "Service Updated", body = UpdateServiceResponse),
        (status = 400, description = "Bad Request", body = ErrorResponse),
        (status = 500, description = "Internal Server Error", body = ErrorResponse),
    ),
    tags = ["Services"],
)]
#[debug_handler]
pub async fn handle_update_service(
    State(state): State<Arc<AppState>>,
    WithRejection(Path(path), _): WithRejection<
        Path<UpdateServicePathInput>,
        ApiResponse<ErrorResponse>,
    >,
    WithRejection(Json(payload), _): WithRejection<
        Json<UpdateServiceInput>,
        ApiResponse<ErrorResponse>,
    >,
) -> Result<ApiResponse<Service>, ApiResponse<()>> {
    if payload.is_enabled.is_none() && payload.location.is_none() && payload.topics.is_none() {
        return Err(bad_request(
            "At least one of 'is_enabled', 'location', or 'topics' is required",
        ))?;
    }

    if payload.location.as_deref() == Some("") {
        return Err(bad_request("Field 'location' cannot be empty"))?;
    }

    if payload
        .topics
        .as_ref()
        .is_some_and(|topics| topics.is_empty())
    {
        return Err(bad_request("Field 'topics' cannot be empty"))?;
    }

    let service = state
        .repo
        .service
        .update_service(
            &path.service_id,
            payload.is_enabled,
            payload.location.as_deref(),
        )
        .await
        .inspect_err(|e| {
            error!("ServiceRepository update service error: {:?}", e);
        })
        .map_err(|_| internal_server_error("Failed to update service"))?;

    if let Some(topics) = payload.topics {
        state
            .repo
            .topic
            .set_service_topics(&service.id, &topics)
            .await
            .inspect_err(|e| {
                error!("TopicRepository set service topics error: {:?}", e);
            })
            .map_err(|_| internal_server_error("Failed to update service topics"))?;
    }

    Ok(ok_response(service))
}
