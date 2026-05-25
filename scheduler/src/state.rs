use std::sync::Arc;

use reqwest_middleware::ClientWithMiddleware;

use crate::{repository::*, service_scaler::ServiceScalerRegistry};

pub struct AppState {
    pub repo: Arc<Repositories>,
    pub service_scaler_registry: Arc<ServiceScalerRegistry>,
    pub acl_client: ClientWithMiddleware,
    pub allow_private_url_validation: bool,
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
            allow_private_url_validation: false,
        }
    }

    pub fn new_allowing_private_url_validation(
        repo: Arc<Repositories>,
        service_scaler_registry: Arc<ServiceScalerRegistry>,
        acl_client: ClientWithMiddleware,
    ) -> Self {
        Self {
            repo,
            service_scaler_registry,
            acl_client,
            allow_private_url_validation: true,
        }
    }
}
