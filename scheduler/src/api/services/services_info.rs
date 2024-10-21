use std::sync::Arc;

use axum::{
    debug_handler,
    extract::{Query, State},
};
use axum_extra::extract::WithRejection;
use serde::{Deserialize, Serialize};
use tracing::{debug, error};

use crate::{api::api_response::*, state::AppState};

#[derive(Deserialize)]
pub struct ServicesScaleInfoInput {
    pub service_name: String,
}

#[derive(Serialize)]
pub struct ServicesInfoResponse {
    pub status: String,
    pub count: u64,
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
) -> Result<ApiResponse<ServicesInfoResponse>, ApiResponse<()>> {
    let scaler_info = state
        .service_scaler
        .get_info(params.service_name)
        .await
        .inspect_err(|e| {
            error!("ServiceScaler get info error: {:?}", e);
        })
        .map_err(|e| internal_server_error(format!("ServiceScaler get info: {:?}", e)))?;

    debug!(
        "Successfully got service info name: {}, instances: {}",
        scaler_info.name, scaler_info.instances
    );

    Ok(ok_response(ServicesInfoResponse {
        status: "ok".to_string(),
        count: scaler_info.instances,
    }))
}
