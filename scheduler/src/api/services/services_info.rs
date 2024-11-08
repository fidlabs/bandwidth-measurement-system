use std::sync::Arc;

use axum::{
    debug_handler,
    extract::{Path, State},
};
use axum_extra::extract::WithRejection;
use serde::{Deserialize, Serialize};
use tracing::{debug, error};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::{api::api_response::*, service_scaler::ServiceScalerInfo, state::AppState};

#[derive(Deserialize, ToSchema, IntoParams)]
pub struct ServicesScaleInfoPathInput {
    pub service_id: Uuid,
}

#[derive(Serialize, ToSchema)]
pub struct ServicesScaleInfoResponse(pub ServiceScalerInfo);

/// Get scaling info about service
#[utoipa::path(
    get,
    path = "/services/{service_id}/scale/info",
    params(ServicesScaleInfoPathInput),
    description = r#"
**Get scaling info about service.**
"#,
    security(
        ("bearer_auth" = [])
    ),
    responses(
        (status = 200, description = "Get Service Scaler Info", body = ServicesScaleInfoResponse),
        (status = 400, description = "Bad Request", body = ErrorResponse),
        (status = 500, description = "Internal Server Error", body = ErrorResponse),
    ),
    tags = ["Services"],
)]
#[debug_handler]
pub async fn handle_services_info(
    WithRejection(Path(path), _): WithRejection<
        Path<ServicesScaleInfoPathInput>,
        ApiResponse<ErrorResponse>,
    >,
    State(state): State<Arc<AppState>>,
) -> Result<ApiResponse<ServiceScalerInfo>, ApiResponse<()>> {
    // Get service from db
    let service = state
        .repo
        .service
        .get_service_by_id(&path.service_id)
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
