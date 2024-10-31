use std::sync::Arc;

use axum::{
    debug_handler,
    extract::{Query, State},
};
use axum_extra::extract::WithRejection;
use serde::Deserialize;
use tracing::{debug, error};
use uuid::Uuid;

use crate::{api::api_response::*, service_scaler::ServiceScalerInfo, state::AppState};

#[derive(Deserialize)]
pub struct ServicesScaleInfoInput {
    pub service_id: Uuid,
}

/// GET /services/info?service_name={service_name}
/// Get info about service
#[debug_handler]
pub async fn handle(
    WithRejection(Query(params), _): WithRejection<
        Query<ServicesScaleInfoInput>,
        ApiResponse<ErrorResponse>,
    >,
    State(state): State<Arc<AppState>>,
) -> Result<ApiResponse<ServiceScalerInfo>, ApiResponse<()>> {
    // Get service from db
    let service = state
        .repo
        .service
        .get_service_by_id(&params.service_id)
        .await
        .inspect_err(|e| {
            error!("ServiceRepository get service error: {:?}", e);
        })
        .map_err(|_| internal_server_error("Failed to get service"))?;

    // Get service info
    let scaler_info = state
        .service_scaler_registry
        .get_scaler(&service.provider_type)
        .ok_or(bad_request("ServiceScaler not found"))?
        .get_info(&service)
        .await
        .inspect_err(|e| {
            error!("ServiceScaler get info error: {:?}", e);
        })
        .map_err(|e| internal_server_error(format!("ServiceScaler get info: {:?}", e)))?;

    debug!(
        "Successfully got service info name: {}, instances: {}",
        scaler_info.name, scaler_info.instances
    );

    Ok(ok_response(scaler_info))
}
