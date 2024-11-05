use std::sync::Arc;

use crate::{repository::*, service_scaler::ServiceScalerRegistry};

pub struct AppState {
    pub repo: Arc<Repositories>,
    pub service_scaler_registry: Arc<ServiceScalerRegistry>,
}

impl AppState {
    pub fn new(
        repo: Arc<Repositories>,
        service_scaler_registry: Arc<ServiceScalerRegistry>,
    ) -> Self {
        Self {
            repo,
            service_scaler_registry,
        }
    }
}
