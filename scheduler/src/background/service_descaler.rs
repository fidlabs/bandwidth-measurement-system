use std::sync::Arc;

use tokio::time::{sleep, Duration};
use tracing::{error, info};

use crate::{service_scaler::ServiceScalerRegistry, Repositories};

const LOOP_DELAY: Duration = Duration::from_secs(60);

pub async fn service_descaler_handler(
    repo: Arc<Repositories>,
    service_scaler_registry: Arc<ServiceScalerRegistry>,
) {
    info!("Starting service descaler handler");

    loop {
        sleep(LOOP_DELAY).await;

        let services = repo
            .service
            .get_services_with_descale_deadline_reached()
            .await
            .map_err(|e| error!("get_services_with_past_scaling_deadline error: {}", e))
            .unwrap_or(vec![]);

        for service in services {
            let scaler = match service_scaler_registry.get_scaler(&service.provider_type) {
                Some(scaler) => scaler,
                None => {
                    error!(
                        "ServiceScalerRegistry missing scaler for provider type: {:?}",
                        &service.provider_type
                    );
                    continue;
                }
            };

            if let Err(e) = scaler.scale_down(&service, i32::MAX).await {
                error!("ServiceScalerError: {:?}", e);
                continue;
            };

            info!("Scaled down service: {}", service.name);

            if let Err(e) = repo.service.clear_descale_deadline(&service.id).await {
                error!("clear_scaling_deadline error: {}", e);
                continue;
            }
        }
    }
}
