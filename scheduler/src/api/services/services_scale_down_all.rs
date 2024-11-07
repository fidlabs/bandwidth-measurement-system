use std::sync::Arc;

use axum::{debug_handler, extract::State};
use serde::Serialize;
use tracing::{debug, error};

use crate::{
    api::api_response::*, service_repository::Service, service_scaler::ServiceScalerInfo,
    state::AppState,
};

#[derive(Debug, Serialize)]
pub struct ServiceWithInfo {
    #[serde(flatten)]
    services: Service,
    info: ServiceScalerInfo,
}

#[derive(Debug, Serialize)]
pub struct ServiceScaleDownAllResponse(pub Vec<ServiceWithInfo>);

/// Scale down all of the services
#[debug_handler]
pub async fn handle(
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
        // Retrieve ServiceScaler from registry by provider type
        let service_scaler = state
            .service_scaler_registry
            .get_scaler(&service.provider_type)
            .ok_or(bad_request("ServiceScaler not found"))?;

        service_scaler
            .scale_down(&service, i32::MAX)
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
                    .clear_descale_deadline(&service.id)
                    .await
                    .inspect_err(|e| {
                        error!("ServiceRepository clear descale at error: {:?}", e);
                    })
                    .map_err(|_| {
                        internal_server_error("Failed to clear descale at for the service")
                    })?;
            }
        }

        debug!(
            "Successfully got service info name: {}, instances: {}",
            service_info.name, service_info.instances
        );

        services_with_info.push(ServiceWithInfo {
            services: service,
            info: service_info,
        });
    }

    Ok(ok_response(ServiceScaleDownAllResponse(services_with_info)))
}
