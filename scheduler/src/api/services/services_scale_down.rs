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
pub struct ServicesScaleDownInput {
    pub service_id: Uuid,
    pub amount: u64,
}

/// Scale down a service by specified amount
#[debug_handler]
pub async fn handle(
    State(state): State<Arc<AppState>>,
    WithRejection(Json(payload), _): WithRejection<
        Json<ServicesScaleDownInput>,
        ApiResponse<ErrorResponse>,
    >,
) -> Result<ApiResponse<ServiceScalerInfo>, ApiResponse<()>> {
    // Get service from db
    let service = state
        .repo
        .service
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

    service_scaler
        .scale_down(&service, payload.amount.try_into().unwrap_or(0))
        .await
        .inspect_err(|e| {
            error!("ServiceScaler scale down error: {:?}", e);
        })
        .map_err(|e| internal_server_error(format!("ServiceScaler scale down: {:?}", e)))?;

    debug!("Successfull worker scale down");

    let service_info = service_scaler
        .get_info(&service)
        .await
        .inspect_err(|e| {
            error!("ServiceScaler get info error: {:?}", e);
        })
        .map_err(|e| internal_server_error(format!("ServiceScaler get info: {:?}", e)))?;

    // Clear descale deadline if desired count is down to 0
    if let Some(desired_count) = service_info.desired_count {
        if desired_count == 0 {
            state
                .repo
                .service
                .clear_descale_deadline(&payload.service_id)
                .await
                .inspect_err(|e| {
                    error!("ServiceRepository clear descale at error: {:?}", e);
                })
                .map_err(|_| internal_server_error("Failed to clear descale at for the service"))?;
        }
    }

    debug!(
        "Successfully got service info name: {}, instances: {}",
        service_info.name, service_info.instances
    );

    Ok(ok_response(service_info))
}
