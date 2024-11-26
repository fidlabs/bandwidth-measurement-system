use std::sync::Arc;

use axum::{debug_handler, extract::State};
use common::api_response::*;
use serde::Serialize;
use tracing::{debug, error};
use utoipa::ToSchema;

use crate::{service_repository::Service, service_scaler::ServiceScalerInfo, state::AppState};

#[derive(Debug, Serialize, ToSchema)]
pub struct ServiceWithInfo {
    #[serde(flatten)]
    services: Service,
    info: Option<ServiceScalerInfo>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ServiceScaleDownAllResponse(pub Vec<ServiceWithInfo>);

/// Scale down all of the services
#[utoipa::path(
    post,
    path = "/services/scale/down/all",
    description = r#"
**Scale down all of the services.**
"#,
    security(
        ("bearer_auth" = [])
    ),
    responses(
        (status = 200, description = "All Services Scalled To Zero", body = ServiceScaleDownAllResponse),
        (status = 400, description = "Bad Request", body = ErrorResponse),
        (status = 500, description = "Internal Server Error", body = ErrorResponse),
    ),
    tags = ["Services"],
)]
#[debug_handler]
pub async fn handle_services_scale_down_all(
    State(state): State<Arc<AppState>>,
) -> Result<ApiResponse<ServiceScaleDownAllResponse>, ApiResponse<()>> {
    let mut services_with_info = vec![];

    // Get service from db
    let services = state
        .repo
        .service
        .get_services()
        .await
        .inspect_err(|e| {
            error!("ServiceRepository get services error: {:?}", e);
        })
        .map_err(|_| internal_server_error("Failed to get services"))?;

    for service in services {
        let mut service_info: Option<ServiceScalerInfo> = None;

        // Retrieve ServiceScaler from registry by provider type
        if let Some(service_scaler) = state
            .service_scaler_registry
            .get_scaler(&service.provider_type)
        {
            service_scaler
                .scale_down(&service, i32::MAX)
                .await
                .inspect_err(|e| {
                    error!("ServiceScaler scale down error: {:?}", e);
                })
                .ok();

            debug!("Successfull worker scale down");

            service_info = service_scaler
                .get_info(&service)
                .await
                .inspect_err(|e| {
                    error!("ServiceScaler get info error: {:?}", e);
                })
                .ok();

            if let Some(s_info) = service_info.as_ref() {
                // Clear descale deadline if desired count is down to 0
                if let Some(desired_count) = s_info.desired_count {
                    if desired_count == 0 {
                        state
                            .repo
                            .service
                            .clear_descale_deadline(&service.id)
                            .await
                            .inspect_err(|e| {
                                error!("ServiceRepository clear descale at error: {:?}", e);
                            })
                            .ok();
                    }
                }
            }
        }

        services_with_info.push(ServiceWithInfo {
            services: service,
            info: service_info,
        });
    }

    Ok(ok_response(ServiceScaleDownAllResponse(services_with_info)))
}
