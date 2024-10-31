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
pub struct DeleteServiceInput {
    pub service_id: Uuid,
}

/// DELETE /services
/// Delete a service
#[debug_handler]
pub async fn handle(
    State(state): State<Arc<AppState>>,
    WithRejection(Json(payload), _): WithRejection<
        Json<DeleteServiceInput>,
        ApiResponse<ErrorResponse>,
    >,
) -> Result<ApiResponse<Service>, ApiResponse<()>> {
    // Create the service
    let service = state
        .repo
        .service
        .delete_service_by_id(&payload.service_id)
        .await
        .inspect_err(|e| {
            error!("ServiceRepository delete service error: {:?}", e);
        })
        .map_err(|_| internal_server_error("Failed to delete service"))?;

    Ok(ok_response(service))
}
