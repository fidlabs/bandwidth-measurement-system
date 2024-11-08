use std::sync::Arc;

use axum::{debug_handler, extract::State};
use serde::Serialize;
use tracing::error;
use utoipa::ToSchema;

use crate::{
    api::api_response::*, service_repository::ServiceWithTopics, service_scaler::ServiceScalerInfo,
    state::AppState,
};

#[derive(Debug, Serialize, ToSchema)]
pub struct GetServicesResponse(pub Vec<ServiceWithTopicsWithInfo>);

#[derive(Debug, Serialize, ToSchema)]
pub struct ServiceWithTopicsWithInfo {
    #[serde(flatten)]
    service: ServiceWithTopics,
    info: Option<ServiceScalerInfo>,
}

/// Get all services with topics and service info from the service scalers
#[utoipa::path(
    get,
    path = "/services",
    description = r#"
**Get all services with topics and service info from the service scalers.**\
"#,
    security(
        ("bearer_auth" = [])
    ),
    responses(
        (status = 200, description = "Get Services", body = GetServicesResponse),
        (status = 400, description = "Bad Request", body = ErrorResponse),
        (status = 500, description = "Internal Server Error", body = ErrorResponse),
    ),
    tags = ["Services"],
)]
#[debug_handler]
pub async fn handle_get_services(
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
        let mut service_info: Option<ServiceScalerInfo> = None;
        if let Some(scaler) = state
            .service_scaler_registry
            .get_scaler(&service.provider_type)
        {
            service_info = scaler
                .get_info(&service.to_service())
                .await
                .inspect_err(|e| {
                    error!(
                        "ServiceScaler get info error for service {}: {:?}",
                        service.name, e
                    );
                })
                .ok();
        }
        services_with_topics_with_info.push(ServiceWithTopicsWithInfo {
            service,
            info: service_info,
        });
    }

    Ok(ok_response(GetServicesResponse(
        services_with_topics_with_info,
    )))
}
