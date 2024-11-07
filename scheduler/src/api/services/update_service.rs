use std::sync::Arc;

use axum::{
    debug_handler,
    extract::{Json, State},
};
use axum_extra::extract::WithRejection;
use serde::Deserialize;
use tracing::error;
use uuid::Uuid;

use crate::{api::api_response::*, service_repository::Service, state::AppState};

#[derive(Deserialize)]
pub struct UpdateServiceInput {
    pub service_id: Uuid,
    pub is_enabled: bool,
}

/// Create a new service and its topics
#[debug_handler]
pub async fn handle(
    State(state): State<Arc<AppState>>,
    WithRejection(Json(payload), _): WithRejection<
        Json<UpdateServiceInput>,
        ApiResponse<ErrorResponse>,
    >,
) -> Result<ApiResponse<Service>, ApiResponse<()>> {
    // Create the service
    let service = state
        .repo
        .service
        .update_service_set_enabled(&payload.service_id, &payload.is_enabled)
        .await
        .inspect_err(|e| {
            error!("ServiceRepository create service error: {:?}", e);
        })
        .map_err(|_| internal_server_error("Failed to create service"))?;

    Ok(ok_response(service))
}
