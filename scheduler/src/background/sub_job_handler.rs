use std::sync::Arc;

use rabbitmq::Publisher;
use tokio::{
    sync::Mutex,
    time::{sleep, Duration},
};
use tracing::{debug, error, info};

use crate::{
    background::{
        sub_job_combineddhp::process_combined_dhp_type, sub_job_scaling::process_scaling,
    },
    service_scaler::ServiceScalerRegistry,
    sub_job_repository::SubJobType,
    Repositories,
};

const LOOP_DELAY: Duration = Duration::from_secs(5);

pub(super) enum SubJobHandlerError {
    Skip(String),
    FailedJob(String),
}

pub async fn sub_job_handler(
    repo: Arc<Repositories>,
    job_queue: Arc<Mutex<Publisher>>,
    service_scaler_registry: Arc<ServiceScalerRegistry>,
) {
    info!("Starting sub job handler");

    loop {
        sleep(LOOP_DELAY).await;

        debug!("Checking for new sub jobs");

        let sub_job = match repo.sub_job.get_first_unfinished_sub_job().await {
            Ok(sub_job) => sub_job,
            Err(e) => {
                debug!("get_first_unfinished_sub_job error: {}", e);
                continue;
            }
        };

        let _ = match sub_job.r#type {
            SubJobType::CombinedDHP => {
                let job_res = repo.job.get_job_by_id(&sub_job.job_id).await;
                if job_res.is_err() {
                    error!("get_job_by_id error: {}", job_res.err().unwrap());
                    continue;
                }
                let job = job_res.unwrap();
                process_combined_dhp_type(repo.clone(), job_queue.clone(), job, sub_job).await
            }
            SubJobType::Scaling => {
                process_scaling(repo.clone(), service_scaler_registry.clone(), sub_job).await
            }
        };
    }
}
