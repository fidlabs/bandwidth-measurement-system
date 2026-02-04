use std::sync::Arc;

use rabbitmq::Publisher;
use tokio::task::JoinSet;
use tokio::time::{sleep, Duration};
use tracing::{debug, error, info};

use crate::{
    background::{
        sub_job_combineddhp::process_combined_dhp_type, sub_job_scaling::process_scaling,
    },
    job_repository::JobType,
    service_scaler::ServiceScalerRegistry,
    sub_job_repository::SubJobType,
    Repositories,
};

const LOOP_DELAY: Duration = Duration::from_secs(5);

pub enum SubJobHandlerError {
    Skip(String),
    FailedJob(String),
}

/// Process one iteration of pending sub-jobs (extracted for testing)
pub async fn process_pending_sub_jobs(
    repo: &Arc<Repositories>,
    job_queue: &Arc<Publisher>,
    service_scaler_registry: &Arc<ServiceScalerRegistry>,
) -> Result<(), SubJobHandlerError> {
    debug!("Checking for new sub jobs");

    // TODO: Job picking is blocking, will not go with NON-overlaping jobs
    // TODO: Consider getting sub job with job join and simplify configuration like MAX_WORKERS
    let sub_job = match repo.sub_job.get_first_unfinished_sub_job().await {
        Ok(sub_job) => sub_job,
        Err(sqlx::Error::RowNotFound) => {
            debug!("No unfinished sub jobs found");
            return Err(SubJobHandlerError::Skip(
                "No unfinished sub jobs".to_string(),
            ));
        }
        Err(e) => {
            error!("get_first_unfinished_sub_job error: {}", e);
            return Err(SubJobHandlerError::Skip(format!("Database error: {}", e)));
        }
    };

    debug!("Found sub job: {:?}", sub_job);

    match (&sub_job.job.job_type, &sub_job.r#type) {
        (JobType::Geolocation, SubJobType::CombinedDHP) => {
            // Geolocation benchmarks: parallel publish for Created, individual processing for others
            match sub_job.status {
                crate::sub_job_repository::SubJobStatus::Created => {
                    // Guard: ensure all scaling is complete before running benchmarks
                    let has_pending_scaling = repo
                        .sub_job
                        .has_pending_scaling_for_job(sub_job.job_id)
                        .await
                        .map_err(|e| SubJobHandlerError::Skip(e.to_string()))?;

                    if has_pending_scaling {
                        return Err(SubJobHandlerError::Skip(
                            "Waiting for scaling to complete".to_string(),
                        ));
                    }

                    // Parallel publishing for all Created geolocation benchmarks
                    info!(
                        "Processing geolocation benchmark sub-jobs in parallel for job {}",
                        sub_job.job_id
                    );

                    let all_benchmarks = match repo
                        .sub_job
                        .get_all_pending_benchmarks_for_job(sub_job.job_id)
                        .await
                    {
                        Ok(benchmarks) => benchmarks,
                        Err(e) => {
                            error!("Failed to get benchmark sub-jobs: {}", e);
                            if let Err(update_err) = repo
                                .sub_job
                                .update_sub_job_status_with_error(
                                    &sub_job.id,
                                    crate::sub_job_repository::SubJobStatus::Failed,
                                    format!("Failed to get benchmark sub-jobs: {}", e),
                                )
                                .await
                            {
                                error!(
                                    "Failed to update sub_job {} status to Failed: {}",
                                    sub_job.id, update_err
                                );
                            }
                            return Err(SubJobHandlerError::FailedJob(format!(
                                "Failed to get benchmarks: {}",
                                e
                            )));
                        }
                    };

                    if all_benchmarks.is_empty() {
                        // Picked a Created sub_job but query returned nothing - data inconsistency
                        error!(
                            "No pending benchmarks found for job {} sub_job {} - this indicates data inconsistency",
                            sub_job.job_id, sub_job.id
                        );
                        if let Err(update_err) = repo
                            .sub_job
                            .update_sub_job_status_with_error(
                                &sub_job.id,
                                crate::sub_job_repository::SubJobStatus::Failed,
                                "No pending benchmarks found for Created sub_job".to_string(),
                            )
                            .await
                        {
                            error!(
                                "Failed to update sub_job {} status to Failed: {}",
                                sub_job.id, update_err
                            );
                        }
                        return Err(SubJobHandlerError::FailedJob(
                            "No pending benchmarks - data inconsistency".to_string(),
                        ));
                    }

                    info!(
                        "Publishing {} benchmark sub-jobs in parallel",
                        all_benchmarks.len()
                    );

                    // Publish all benchmarks in parallel
                    let mut join_set = JoinSet::new();
                    for benchmark in all_benchmarks {
                        let repo = repo.clone();
                        let job_queue = job_queue.clone();
                        join_set.spawn(async move {
                            process_combined_dhp_type(repo, job_queue, benchmark).await
                        });
                    }

                    while let Some(result) = join_set.join_next().await {
                        match result {
                            Ok(Ok(_)) => {}
                            Ok(Err(e)) => error!("Failed to process benchmark sub-job: {:?}", e),
                            Err(e) => error!("Benchmark task panicked: {:?}", e),
                        }
                    }

                    Ok(())
                }
                _ => {
                    // Pending/Processing - process individually for deadline and completion checks
                    match process_combined_dhp_type(repo.clone(), job_queue.clone(), sub_job).await
                    {
                        Ok(_) => Ok(()),
                        Err(e) => Err(SubJobHandlerError::FailedJob(format!(
                            "CombinedDHP processing failed: {}",
                            e
                        ))),
                    }
                }
            }
        }
        (_, SubJobType::CombinedDHP) => {
            // Sequential processing for bandwidth_saturation
            match process_combined_dhp_type(repo.clone(), job_queue.clone(), sub_job).await {
                Ok(_) => Ok(()),
                Err(e) => Err(SubJobHandlerError::FailedJob(format!(
                    "CombinedDHP processing failed: {}",
                    e
                ))),
            }
        }
        (JobType::Geolocation, SubJobType::Scaling) => {
            // Get all scaling sub-jobs for this geolocation job
            let all_scaling = repo
                .sub_job
                .get_all_pending_scaling_for_job(sub_job.job_id)
                .await
                .map_err(|e| SubJobHandlerError::Skip(e.to_string()))?;

            if all_scaling.is_empty() {
                return Err(SubJobHandlerError::Skip(
                    "No pending scaling sub-jobs found".to_string(),
                ));
            }

            info!(
                "Processing {} geolocation scaling sub-jobs in parallel for job {}",
                all_scaling.len(),
                sub_job.job_id
            );

            let mut success_count = 0;
            let mut failure_count = 0;

            let mut join_set = JoinSet::new();
            for scaling_sub_job in all_scaling {
                let repo = repo.clone();
                let scaler = service_scaler_registry.clone();
                join_set.spawn(async move { process_scaling(repo, scaler, scaling_sub_job).await });
            }

            while let Some(result) = join_set.join_next().await {
                match result {
                    Ok(Ok(_)) => success_count += 1,
                    Ok(Err(e)) => {
                        error!("Geolocation scaling sub-job failed: {:?}", e);
                        failure_count += 1;
                    }
                    Err(e) => {
                        error!("Geolocation scaling task panicked: {:?}", e);
                        failure_count += 1;
                    }
                }
            }

            info!(
                "Geolocation scaling complete for job {}: {} succeeded, {} failed",
                sub_job.job_id, success_count, failure_count
            );

            Ok(())
        }
        (_, SubJobType::Scaling) => {
            match process_scaling(repo.clone(), service_scaler_registry.clone(), sub_job).await {
                Ok(_) => Ok(()),
                Err(e) => Err(SubJobHandlerError::FailedJob(format!(
                    "Scaling failed: {}",
                    e
                ))),
            }
        }
    }
}

/// Background loop that continuously processes sub-jobs
pub async fn sub_job_handler(
    repo: Arc<Repositories>,
    job_queue: Arc<Publisher>,
    service_scaler_registry: Arc<ServiceScalerRegistry>,
) {
    info!("Starting sub job handler");

    loop {
        sleep(LOOP_DELAY).await;
        let _ = process_pending_sub_jobs(&repo, &job_queue, &service_scaler_registry).await;
    }
}
