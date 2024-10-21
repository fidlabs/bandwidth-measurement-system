use std::sync::Arc;

use amqprs::{
    channel::{BasicAckArguments, Channel},
    consumer::AsyncConsumer,
    BasicProperties, Deliver,
};
use async_trait::async_trait;
use color_eyre::{eyre::eyre, Result};
use rabbitmq::{Message, ResultMessage};
use serde_json;
use tracing::{debug, error, info};
use uuid::Uuid;

use crate::{job_repository::JobStatus, state::AppState, sub_job_repository::SubJobStatus};

pub struct DataConsumer {
    state: Arc<AppState>,
}

impl DataConsumer {
    pub fn new(state: Arc<AppState>) -> Self {
        Self { state }
    }

    async fn parse_message(&self, content_str: &str) -> Result<(Uuid, ResultMessage)> {
        match serde_json::from_str::<Message>(content_str) {
            Ok(Message::WorkerResult { job_id, result }) => Ok((job_id, result)),
            Ok(_) => Err(eyre!("Received unexpected message")),
            Err(e) => {
                error!("Error parsing message: {:?}", e);
                Err(e.into())
            }
        }
    }

    #[tracing::instrument(skip(self, result_message), fields(worker_name = %result_message.worker_name))]
    async fn process_message(&self, job_id: Uuid, result_message: ResultMessage) -> Result<()> {
        info!("Handling data message");
        debug!("Handling data message: {:?} {:?}", job_id, result_message);

        let sub_job_id = result_message.sub_job_id;
        let is_success = result_message.is_success;

        // Save the data
        self.state.data_repo.save_data(result_message).await?;
        // Update the sub job status
        self.state
            .sub_job_repo
            .update_sub_job_status(
                &sub_job_id,
                if is_success {
                    SubJobStatus::Completed
                } else {
                    SubJobStatus::Failed
                },
            )
            .await?;

        // Check if all sub jobs are completed
        let pending_sub_jobs = self
            .state
            .sub_job_repo
            .count_pending_sub_jobs(job_id)
            .await?;
        debug!("Pending sub jobs: {}", pending_sub_jobs);

        // Update the job status if all sub jobs are completed
        if pending_sub_jobs == 0 {
            debug!("All sub jobs completed for job_id: {}", job_id);

            self.state
                .job_repo
                .update_job_status(job_id, JobStatus::Completed)
                .await?;
        }

        Ok(())
    }

    async fn run(&self, content: Vec<u8>) -> Result<()> {
        let content_str = String::from_utf8(content)?;

        debug!("Received message: {}", content_str);

        let (job_id, result_message) = self.parse_message(&content_str).await?;

        self.process_message(job_id, result_message).await?;

        Ok(())
    }
}

#[async_trait]
impl AsyncConsumer for DataConsumer {
    async fn consume(
        &mut self,
        channel: &Channel,
        deliver: Deliver,
        _basic_properties: BasicProperties,
        content: Vec<u8>,
    ) {
        match self.run(content).await {
            Ok(_) => {
                info!("Processed message successfully");
                // Ack message only if processed successfully
                let args = BasicAckArguments::new(deliver.delivery_tag(), false);
                channel.basic_ack(args).await.unwrap();
                debug!("Acked message");
            }
            Err(e) => {
                error!("Error processing message: {:?}", e);
            }
        }
    }
}
