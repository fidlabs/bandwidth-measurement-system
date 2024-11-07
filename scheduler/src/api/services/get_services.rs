use std::sync::Arc;

use axum::{debug_handler, extract::State};
use serde::Serialize;
use tracing::error;

use crate::{
    api::api_response::*, service_repository::ServiceWithTopics, service_scaler::ServiceScalerInfo,
    state::AppState,
};

#[derive(Debug, Serialize)]
pub struct GetServicesResponse(pub Vec<ServiceWithTopicsWithInfo>);

#[derive(Debug, Serialize)]
pub struct ServiceWithTopicsWithInfo {
    #[serde(flatten)]
    service: ServiceWithTopics,
    info: ServiceScalerInfo,
}

/// Get all services with topics and service info from the service scalers
#[debug_handler]
pub async fn handle(
    State(state): State<Arc<AppState>>,
) -> Result<ApiResponse<GetServicesResponse>, ApiResponse<()>> {
    // Get all services
    let services = state
        .repo
        .service
        .get_services_with_topics()
        .await
        .inspect_err(|e| {
            error!("ServiceRepository get services error: {:?}", e);
        })
        .map_err(|_| internal_server_error("Failed to get services"))?;

    let mut services_with_topics_with_info = vec![];
    for service in services {
        let scaler_info = state
            .service_scaler_registry
            .get_scaler(&service.provider_type)
            .ok_or(bad_request("ServiceScaler not found"))?
            .get_info(&service.to_service())
            .await
            .inspect_err(|e| {
                error!("ServiceScaler get info error: {:?}", e);
            })
            .map_err(|e| internal_server_error(format!("ServiceScaler get info: {:?}", e)))?;

        services_with_topics_with_info.push(ServiceWithTopicsWithInfo {
            service,
            info: scaler_info,
        });
    }

    Ok(ok_response(GetServicesResponse(
        services_with_topics_with_info,
    )))
}
