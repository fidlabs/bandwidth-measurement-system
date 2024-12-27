use std::sync::Arc;

use rabbitmq::Publisher;
use tokio::time::{sleep, Duration};
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
    job_queue: Arc<Publisher>,
    service_scaler_registry: Arc<ServiceScalerRegistry>,
) {
    info!("Starting sub job handler");

    loop {
        sleep(LOOP_DELAY).await;

        debug!("Checking for new sub jobs");

        // TODO: Job picking is blocking, will not go with NON-overlaping jobs
        // TODO: Consider getting sub job with job join and simplify configuration like MAX_WORKERS
        let sub_job = match repo.sub_job.get_first_unfinished_sub_job().await {
            Ok(sub_job) => sub_job,
            Err(sqlx::Error::RowNotFound) => {
                debug!("No unfinished sub jobs found");
                continue;
            }
            Err(e) => {
                error!("get_first_unfinished_sub_job error: {}", e);
                continue;
            }
        };

        debug!("Found sub job: {:?}", sub_job);

        let _ = match sub_job.r#type {
            SubJobType::CombinedDHP => {
                process_combined_dhp_type(repo.clone(), job_queue.clone(), sub_job).await
            }
            SubJobType::Scaling => {
                process_scaling(repo.clone(), service_scaler_registry.clone(), sub_job).await
            }
        };
    }
}
