use std::sync::Arc;

use axum::{
    debug_handler,
    extract::{Json, State},
};
use axum_extra::extract::WithRejection;
use serde::Deserialize;
use tracing::{debug, error};
use uuid::Uuid;

use crate::{api::api_response::*, service_scaler::ServiceScalerInfo, state::AppState};

#[derive(Deserialize)]
pub struct ServicesScaleUpInput {
    pub service_id: Uuid,
    pub amount: u64,
}

/// GET /services/scale/up
/// Scale up a service
#[debug_handler]
pub async fn handle(
    State(state): State<Arc<AppState>>,
    WithRejection(Json(payload), _): WithRejection<
        Json<ServicesScaleUpInput>,
        ApiResponse<ErrorResponse>,
    >,
) -> Result<ApiResponse<ServiceScalerInfo>, ApiResponse<()>> {
    // Get service from db
    let service = state
        .service_repo
        .get_service_by_id(&payload.service_id)
        .await
        .inspect_err(|e| {
            error!("ServiceRepository get service error: {:?}", e);
        })
        .map_err(|_| internal_server_error("Failed to get service"))?;

    // Retrieve ServiceScaler from registry by provider type
    let service_scaler = state
        .service_scaler_registry
        .get_scaler(&service.provider_type)
        .ok_or(bad_request("ServiceScaler not found"))?;

    // Scale up the service
    service_scaler
        .scale_up(&service, payload.amount)
        .await
        .inspect_err(|e| {
            error!("ServiceScaler scale up error: {:?}", e);
        })
        .map_err(|e| internal_server_error(format!("ServiceScaler scale up: {:?}", e)))?;

    debug!("Successfull worker scale up");

    // Get service info
    let service_info = service_scaler
        .get_info(&service)
        .await
        .inspect_err(|e| {
            error!("ServiceScaler get info error: {:?}", e);
        })
        .map_err(|e| internal_server_error(format!("ServiceScaler get info: {:?}", e)))?;

    debug!(
        "Successfully got service info name: {}, instances: {}",
        service_info.name, service_info.instances
    );

    Ok(ok_response(service_info))
}
