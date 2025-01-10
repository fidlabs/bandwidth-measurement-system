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
    pub is_enabled: bool,
}

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
    // Create the service
    let service = state
        .repo
        .service
        .update_service_set_enabled(&path.service_id, &payload.is_enabled)
        .await
        .inspect_err(|e| {
            error!("ServiceRepository create service error: {:?}", e);
        })
        .map_err(|_| internal_server_error("Failed to create service"))?;

    Ok(ok_response(service))
}
