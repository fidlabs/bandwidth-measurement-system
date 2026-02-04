use std::sync::Arc;

use reqwest_middleware::ClientWithMiddleware;

use crate::{repository::*, service_scaler::ServiceScalerRegistry};

pub struct AppState {
    pub repo: Arc<Repositories>,
    pub service_scaler_registry: Arc<ServiceScalerRegistry>,
    pub acl_client: ClientWithMiddleware,
}

impl AppState {
    pub fn new(
        repo: Arc<Repositories>,
        service_scaler_registry: Arc<ServiceScalerRegistry>,
        acl_client: ClientWithMiddleware,
    ) -> Self {
        Self {
            repo,
            service_scaler_registry,
            acl_client,
        }
    }
}
