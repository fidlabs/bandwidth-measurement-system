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
pub struct ServicesScaleDownPathInput {
    pub service_id: Uuid,
}
#[derive(Deserialize, ToSchema)]
pub struct ServicesScaleDownInput {
    pub amount: u64,
}

#[derive(Serialize, ToSchema)]
pub struct ServiceScaleDownResponse(pub ServiceScalerInfo);

/// Scale down a service by specified amount
#[utoipa::path(
    post,
    path = "/services/{service_id}/scale/down",
    params(ServicesScaleDownPathInput),
    request_body(content = ServicesScaleDownInput),
    description = r#"
**Scale down a service by specified amount.**
"#,
    security(
        ("bearer_auth" = [])
    ),
    responses(
        (status = 200, description = "Services Scaled Down", body = ServiceScaleDownResponse),
        (status = 400, description = "Bad Request", body = ErrorResponse),
        (status = 500, description = "Internal Server Error", body = ErrorResponse),
    ),
    tags = ["Services"],
)]
#[debug_handler]
pub async fn handle_services_scale_down(
    State(state): State<Arc<AppState>>,
    WithRejection(Path(path), _): WithRejection<
        Path<ServicesScaleDownPathInput>,
        ApiResponse<ErrorResponse>,
    >,
    WithRejection(Json(payload), _): WithRejection<
        Json<ServicesScaleDownInput>,
        ApiResponse<ErrorResponse>,
    >,
) -> Result<ApiResponse<ServiceScaleDownResponse>, ApiResponse<()>> {
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
                .clear_descale_deadline(&path.service_id)
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

    Ok(ok_response(ServiceScaleDownResponse(service_info)))
}
