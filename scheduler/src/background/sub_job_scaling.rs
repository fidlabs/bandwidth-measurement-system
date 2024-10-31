use std::sync::Arc;

use chrono::Utc;
use color_eyre::Result;
use tokio::time::Duration;
use tracing::{debug, error, info};

use crate::{
    service_repository::Service,
    service_scaler::ServiceScalerRegistry,
    sub_job_repository::{SubJob, SubJobStatus},
    Repositories,
};

use super::sub_job_handler::SubJobHandlerError;

const SERVICE_DEADLINE_SEC: u64 = 3600; // 1 hour
const MAX_JOB_WORKERS: usize = 10;

pub(super) async fn process_scaling(
    repo: Arc<Repositories>,
    service_scaler_registry: Arc<ServiceScalerRegistry>,
    sub_job: SubJob,
) -> Result<()> {
    let result = match sub_job.status {
        SubJobStatus::Created => {
            process_scaling_created(repo.clone(), service_scaler_registry, &sub_job).await
        }
        SubJobStatus::Pending => process_scaling_pending(&sub_job).await,
        SubJobStatus::Processing => process_scaling_processing(repo.clone(), &sub_job).await,
        _ => Err(SubJobHandlerError::FailedJob(
            "Invalid and unexpected status".to_string(),
        )),
    };

    match result {
        Ok(_) => {}
        Err(SubJobHandlerError::Skip(e)) => {
            info!("SubJobScalingError::Skip: {}", e);
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

#[tracing::instrument(skip(repo, service_scaler_registry, sub_job), fields(sub_job_id = %sub_job.id))]
async fn process_scaling_created(
    repo: Arc<Repositories>,
    service_scaler_registry: Arc<ServiceScalerRegistry>,
    sub_job: &SubJob,
) -> Result<(), SubJobHandlerError> {
    info!("Processing scaling created type sub job");

    let topic = get_topic(sub_job)?;
    let services = get_services(repo.clone(), &topic).await?;
    let workers_online = get_workers_online(repo.clone(), &topic).await?;
    let service_count = services.len();

    let scale_each_by = MAX_JOB_WORKERS.div_ceil(service_count);

    // Set expected worker count into the job details
    let workers_count = (scale_each_by * service_count) as i64;
    repo.job
        .update_job_workers_count(&sub_job.job_id, workers_count)
        .await
        .map_err(|e| SubJobHandlerError::Skip(e.to_string()))?;

    // Extend the descale_at deadline for all services
    let descale_at = Utc::now() + Duration::from_secs(SERVICE_DEADLINE_SEC);
    repo.service
        .set_descale_deadlines(&services.iter().map(|s| s.id).collect(), descale_at)
        .await
        .map_err(|e| SubJobHandlerError::Skip(e.to_string()))?;

    debug!(
        "workers_online: {} services: {:?}, scale_each_by: {}, service_count: {}, workers_count: {}, descale_at: {}",
        workers_online.len(),
        services.iter().map(|s| &s.id),
        scale_each_by,
        service_count,
        workers_count,
        descale_at
    );

    // Do not scale if workers are already online
    // Set the sub job status to canceled
    if workers_online.len() >= service_count {
        repo.sub_job
            .update_sub_job_status(&sub_job.id, SubJobStatus::Canceled)
            .await
            .map_err(|e| SubJobHandlerError::Skip(e.to_string()))?;

        return Ok(());
    }

    //  TODO: we are lacking a diff...
    //  service picker doesnt care about even distribution of services by the topic
    //  if we run poland that has europe topic then europe topic will skip, and will not spawn additional instances or the rest of services with europe topic
    //  also we do not count diff of online and max versus missing workers, that might spawn more workers than we need

    // Scale up each service
    for service in services {
        let scaler = service_scaler_registry
            .get_scaler(&service.provider_type)
            .ok_or(SubJobHandlerError::FailedJob("No scaler found".to_string()))?;

        scaler
            .scale_up(&service, scale_each_by as u64)
            .await
            .map_err(|e| SubJobHandlerError::Skip(e.to_str()))?;
    }

    // Update sub job status to processing
    repo.sub_job
        .update_sub_job_status(&sub_job.id, SubJobStatus::Processing)
        .await
        .map_err(|e| SubJobHandlerError::Skip(e.to_string()))?;

    Ok(())
}

#[tracing::instrument(skip( sub_job), fields(sub_job_id = %sub_job.id))]
async fn process_scaling_pending(sub_job: &SubJob) -> Result<(), SubJobHandlerError> {
    error!("Processing scaling pending type sub job");

    Err(SubJobHandlerError::FailedJob(
        "Invalid pending state".to_string(),
    ))
}

#[tracing::instrument(skip(repo, sub_job), fields(sub_job_id = %sub_job.id))]
async fn process_scaling_processing(
    repo: Arc<Repositories>,
    sub_job: &SubJob,
) -> Result<(), SubJobHandlerError> {
    info!("Processing scaling processing type sub job");

    let topic = get_topic(sub_job)?;
    let services = get_services(repo.clone(), &topic).await?;
    let scale_each_by = MAX_JOB_WORKERS.div_ceil(services.len());
    let workers_online = get_workers_online(repo.clone(), &topic).await?;

    debug!(
        "workers_online: {}, scale_each_by: {} services: {}",
        workers_online.len(),
        scale_each_by,
        services.len()
    );

    let workers_count = scale_each_by * services.len();

    if workers_online.len() >= workers_count {
        repo.sub_job
            .update_sub_job_status(&sub_job.id, SubJobStatus::Completed)
            .await
            .map_err(|e| SubJobHandlerError::Skip(e.to_string()))?;
    }

    Ok(())
}

fn get_topic(sub_job: &SubJob) -> Result<String, SubJobHandlerError> {
    let topic = sub_job
        .details
        .get("topic")
        .ok_or(SubJobHandlerError::FailedJob(
            "missing service topic".to_string(),
        ))?
        .as_str()
        .unwrap()
        .to_string();

    Ok(topic)
}

async fn get_services(
    repo: Arc<Repositories>,
    topic: &str,
) -> Result<Vec<Service>, SubJobHandlerError> {
    let services = repo
        .service
        .get_services_by_topic(topic)
        .await
        .map_err(|e| SubJobHandlerError::FailedJob(e.to_string()))?;

    if services.is_empty() {
        return Err(SubJobHandlerError::FailedJob(
            "No services found".to_string(),
        ));
    }
    Ok(services)
}

async fn get_workers_online(
    repo: Arc<Repositories>,
    topic: &String,
) -> Result<Vec<String>, SubJobHandlerError> {
    let workers = repo
        .worker
        .get_workers_online_with_topic(topic)
        .await
        .map_err(|e| SubJobHandlerError::Skip(e.to_string()))?;

    Ok(workers)
}
