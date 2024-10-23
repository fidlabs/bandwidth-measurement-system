use std::sync::Arc;

use axum::{
    debug_handler,
    extract::{Json, State},
};
use axum_extra::extract::WithRejection;
use serde::{Deserialize, Serialize};
use tracing::{debug, error};

use crate::{api::api_response::*, state::AppState};

#[derive(Serialize)]
pub struct ServicesScaleDownResponse {
    pub name: String,
    pub instances: u64,
}

#[derive(Deserialize)]
pub struct ServicesScaleDownInput {
    pub service_name: String,
    pub amount: u64,
}

/// POST /services/scale/down
/// Scale down a service
#[debug_handler]
pub async fn handle(
    State(state): State<Arc<AppState>>,
    WithRejection(Json(payload), _): WithRejection<
        Json<ServicesScaleDownInput>,
        ApiResponse<ErrorResponse>,
    >,
) -> Result<ApiResponse<ServicesScaleDownResponse>, ApiResponse<()>> {
    state
        .service_scaler
        .scale_down(payload.service_name.clone(), payload.amount)
        .await
        .inspect_err(|e| {
            error!("ServiceScaler scale down error: {:?}", e);
        })
        .map_err(|e| internal_server_error(format!("ServiceScaler scale down: {:?}", e)))?;

    debug!("Successfull worker scale down");

    let service_info = state
        .service_scaler
        .get_info(payload.service_name)
        .await
        .inspect_err(|e| {
            error!("ServiceScaler get info error: {:?}", e);
        })
        .map_err(|e| internal_server_error(format!("ServiceScaler get info: {:?}", e)))?;

    debug!(
        "Successfully got service info name: {}, instances: {}",
        service_info.name, service_info.instances
    );

    Ok(ok_response(ServicesScaleDownResponse {
        name: service_info.name,
        instances: service_info.instances,
    }))
}
