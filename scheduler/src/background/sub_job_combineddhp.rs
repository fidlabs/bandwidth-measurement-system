use std::sync::Arc;

use chrono::{Duration, Utc};
use color_eyre::Result;
use rabbitmq::{JobMessage, Message, Publisher};
use tokio::sync::Mutex;
use tracing::{debug, error};

use crate::{
    job_repository::{Job, JobStatus},
    sub_job_repository::{SubJob, SubJobStatus, SubJobType},
    Repositories,
};

use super::sub_job_handler::SubJobHandlerError;

const MAX_DOWNLOAD_DURATION_SECS: i64 = 60;
const DOWNLOAD_DELAY_SECS: i64 = 10;
const SYNC_DELAY_SECS: i64 = 1;

/// Process the sub job with type CombinedDHP
pub(super) async fn process_combined_dhp_type(
    repo: Arc<Repositories>,
    job_queue: Arc<Mutex<Publisher>>,
    job: Job,
    sub_job: SubJob,
) -> Result<()> {
    let result = match sub_job.status {
        SubJobStatus::Created => {
            process_status_created(repo.clone(), job_queue.clone(), job, &sub_job).await
        }
        SubJobStatus::Pending => process_status_pending(&sub_job).await,
        SubJobStatus::Processing => process_status_processing(repo.clone(), &sub_job).await,
        _ => Ok(()),
    };

    // TODO: consider moving this to parent function
    match result {
        Ok(_) => {}
        Err(SubJobHandlerError::Skip(e)) => {
            debug!("SubJobScalingError::Skip: {}", e);
        }
        Err(SubJobHandlerError::FailedJob(e)) => {
            error!("ubJobScalingError::FailedJob: {}", e);

            let _ = repo
                .sub_job
                .update_sub_job_status_with_error(&sub_job.id, SubJobStatus::Failed, e)
                .await;
        }
    }

    Ok(())
}

async fn process_status_created(
    repo: Arc<Repositories>,
    job_queue: Arc<Mutex<Publisher>>,
    job: Job,
    sub_job: &SubJob,
) -> Result<(), SubJobHandlerError> {
    // Calculate the start time for the sub jobs
    let start_time = Utc::now() + Duration::seconds(SYNC_DELAY_SECS);
    let download_start_time = start_time + Duration::seconds(DOWNLOAD_DELAY_SECS);

    let job_message = Message::WorkerJob {
        job_id: job.id,
        payload: JobMessage {
            job_id: job.id,
            sub_job_id: sub_job.id,
            url: job.url.clone(),
            start_time,
            download_start_time,
            start_range: job.details.start_range,
            end_range: job.details.end_range,
        },
    };

    debug!("Publishing job message: {:?}", job_message);

    job_queue
        .lock()
        .await
        .publish(&job_message, &job.routing_key)
        .await
        .inspect_err(|e| error!("Failed to publish job message: {}", e))
        .map_err(|_| SubJobHandlerError::Skip("Failed to publish job message".to_string()))?;

    debug!("Job message published successfully: {}", sub_job.id);

    let deadline_at = download_start_time + Duration::seconds(MAX_DOWNLOAD_DURATION_SECS * 2);

    repo.sub_job
        .update_sub_job_status_and_deadline(&sub_job.id, SubJobStatus::Pending, deadline_at)
        .await
        .map_err(|e| SubJobHandlerError::FailedJob(e.to_string()))?;

    Ok(())
}

/// Sub jobs with status pending has been sent to the worker
async fn process_status_pending(sub_job: &SubJob) -> Result<(), SubJobHandlerError> {
    check_deadline(sub_job)?;

    Ok(())
}

// Sub jobs with status processing is started by a worker and is waiting for the data
async fn process_status_processing(
    repo: Arc<Repositories>,
    sub_job: &SubJob,
) -> Result<(), SubJobHandlerError> {
    check_deadline(sub_job)?;

    let job = repo
        .job
        .get_job_by_id(&sub_job.job_id)
        .await
        .map_err(|e| SubJobHandlerError::Skip(format!("Failed to get job: {}", e)))?;

    let workers_count = job
        .details
        .workers_count
        .ok_or(SubJobHandlerError::FailedJob(
            "missing workers count".to_string(),
        ))?;

    let data = repo
        .data
        .get_data_by_sub_job_id(&sub_job.id)
        .await
        .map_err(|e| SubJobHandlerError::Skip(format!("Failed to get data: {}", e)))?;

    if data.len() >= workers_count as usize {
        repo.sub_job
            .update_sub_job_status(&sub_job.id, SubJobStatus::Completed)
            .await
            .map_err(|e| SubJobHandlerError::Skip(e.to_string()))?;

        // Check if all sub jobs are completed
        let pending_sub_jobs = repo
            .sub_job
            .count_pending_sub_jobs(SubJobType::CombinedDHP, &sub_job.job_id)
            .await
            .map_err(|e| {
                SubJobHandlerError::Skip(format!("Failed to count pending sub jobs: {}", e))
            })?;

        debug!("Pending sub jobs: {}", pending_sub_jobs);

        // Update the job status if all sub jobs are completed
        if pending_sub_jobs == 0 {
            debug!("All sub jobs completed for job_id: {}", &sub_job.job_id);

            repo.job
                .update_job_status(&sub_job.job_id, JobStatus::Completed)
                .await
                .map_err(|e| {
                    SubJobHandlerError::Skip(format!("Failed to update job status: {}", e))
                })?;
        }
    }

    Ok(())
}

fn check_deadline(sub_job: &SubJob) -> Result<(), SubJobHandlerError> {
    let deadline = sub_job
        .deadline_at
        .ok_or(SubJobHandlerError::FailedJob("No deadline".to_string()))?;

    if Utc::now() > deadline {
        return Err(SubJobHandlerError::FailedJob("Deadline passed".to_string()));
    }
    Ok(())
}