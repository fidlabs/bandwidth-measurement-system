use std::sync::Arc;

use axum::{debug_handler, extract::State};
use tracing::error;

use crate::{api::api_response::*, service_repository::Service, state::AppState};

/// GET /services
/// Get all services
#[debug_handler]
pub async fn handle(
    State(state): State<Arc<AppState>>,
) -> Result<ApiResponse<Vec<Service>>, ApiResponse<()>> {
    // Get all services
    let services = state
        .service_repo
        .get_services()
        .await
        .inspect_err(|e| {
            error!("ServiceRepository get services error: {:?}", e);
        })
        .map_err(|_| internal_server_error("Failed to get services"))?;

    Ok(ok_response(services))
}
