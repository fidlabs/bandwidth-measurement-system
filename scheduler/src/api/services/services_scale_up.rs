use std::sync::Arc;

use axum::{
    debug_handler,
    extract::{Json, Path, State},
};
use axum_extra::extract::WithRejection;
use common::api_response::*;
use serde::{Deserialize, Serialize};
use tracing::{debug, error};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::{service_scaler::ServiceScalerInfo, state::AppState};

#[derive(Deserialize, ToSchema, IntoParams)]
pub struct ServicesScaleUpPathInput {
    pub service_id: Uuid,
}

#[derive(Deserialize, ToSchema)]
pub struct ServicesScaleUpInput {
    pub amount: u64,
}

#[derive(Serialize, ToSchema)]
pub struct ServiceScaleUpResponse(pub ServiceScalerInfo);

/// Scale up a service by specified amount
#[utoipa::path(
    post,
    path = "/services/{service_id}/scale/up",
    params(ServicesScaleUpPathInput),
    request_body(content = ServicesScaleUpInput),
    description = r#"
**Scale up a service by specified amount.**
"#,
    security(
        ("bearer_auth" = [])
    ),
    responses(
        (status = 200, description = "Services Scaled Up", body = ServiceScaleUpResponse),
        (status = 400, description = "Bad Request", body = ErrorResponse),
        (status = 500, description = "Internal Server Error", body = ErrorResponse),
    ),
    tags = ["Services"],
)]
#[debug_handler]
pub async fn handle_services_scale_up(
    State(state): State<Arc<AppState>>,
    WithRejection(Path(path), _): WithRejection<
        Path<ServicesScaleUpPathInput>,
        ApiResponse<ErrorResponse>,
    >,
    WithRejection(Json(payload), _): WithRejection<
        Json<ServicesScaleUpInput>,
        ApiResponse<ErrorResponse>,
    >,
) -> Result<ApiResponse<ServiceScaleUpResponse>, ApiResponse<()>> {
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

    // Retrieve ServiceScaler from registry by provider type
    let service_scaler = state
        .service_scaler_registry
        .get_scaler(&service.provider_type)
        .ok_or(bad_request("ServiceScaler not found"))?;

    // Scale up the service
    service_scaler
        .scale_up(&service, payload.amount.try_into().unwrap_or(0))
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

    Ok(ok_response(ServiceScaleUpResponse(service_info)))
}
