use std::sync::Arc;

use tokio::time::{sleep, Duration};
use tracing::{error, info};

use crate::Repositories;

const LOOP_DELAY: Duration = Duration::from_secs(60);

/// Check if the worker is online and set it offline if it is not
pub async fn process_worker_online_check(repo: Arc<Repositories>) {
    info!("Starting inactive worker online check");

    loop {
        sleep(LOOP_DELAY).await;

        let workers = repo
            .worker
            .get_inactive_online_workers()
            .await
            .map_err(|e| error!("get_workers error: {}", e))
            .unwrap_or(vec![]);

        for worker_name in workers {
            let _ = repo.worker.set_worker_offline(&worker_name).await;

            info!("Worker: {} is set offline", worker_name);
        }
    }
}
