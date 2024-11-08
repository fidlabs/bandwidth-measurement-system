use std::sync::Arc;

use axum::{
    debug_handler,
    extract::{Path, State},
};
use axum_extra::extract::WithRejection;
use serde::{Deserialize, Serialize};
use tracing::error;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::{api::api_response::*, service_repository::Service, state::AppState};

#[derive(Deserialize, ToSchema, IntoParams)]
pub struct DeleteServicePathInput {
    pub service_id: Uuid,
}

#[derive(Serialize, ToSchema)]
pub struct DeleteServiceResponse(pub Service);

/// Delete a service
#[utoipa::path(
    delete,
    path = "/services/{service_id}",
    params(DeleteServicePathInput),
    description = r#"
**Delete a service.**
"#,
    security(
        ("bearer_auth" = [])
    ),
    responses(
        (status = 200, description = "Deleted Service", body = DeleteServiceResponse),
        (status = 400, description = "Bad Request", body = ErrorResponse),
        (status = 500, description = "Internal Server Error", body = ErrorResponse),
    ),
    tags = ["Services"],
)]
#[debug_handler]
pub async fn handle_delete_service(
    State(state): State<Arc<AppState>>,
    WithRejection(Path(path), _): WithRejection<
        Path<DeleteServicePathInput>,
        ApiResponse<ErrorResponse>,
    >,
) -> Result<ApiResponse<DeleteServiceResponse>, ApiResponse<()>> {
    // Get the service
    let service = state
        .repo
        .service
        .get_service_by_id(&path.service_id)
        .await
        .inspect_err(|e| {
            error!("ServiceRepository get service error: {:?}", e);
        })
        .map_err(|_| internal_server_error("Failed to get service"))?;

    // Scale down the service
    if let Some(scaler) = state
        .service_scaler_registry
        .get_scaler(&service.provider_type)
    {
        scaler
            .scale_down(&service, i32::MAX)
            .await
            .inspect_err(|e| {
                error!(
                    "ServiceScaler get info error for service {}: {:?}",
                    service.name, e
                );
            })
            .ok();
    }

    // Delete the service
    state
        .repo
        .service
        .delete_service_by_id(&path.service_id)
        .await
        .inspect_err(|e| {
            error!("ServiceRepository delete service error: {:?}", e);
        })
        .map_err(|_| internal_server_error("Failed to delete service"))?;

    Ok(ok_response(DeleteServiceResponse(service)))
}
