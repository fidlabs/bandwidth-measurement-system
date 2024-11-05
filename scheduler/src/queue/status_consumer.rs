use std::sync::Arc;

use crate::{state::AppState, sub_job_repository::SubJobStatus};
use amqprs::{
    channel::{BasicAckArguments, Channel},
    consumer::AsyncConsumer,
    BasicProperties, Deliver,
};
use async_trait::async_trait;
use color_eyre::{eyre::eyre, Result};
use rabbitmq::{Message, StatusMessage, WorkerStatus, WorkerStatusDetails};
use serde_json;
use tracing::{debug, error, info};

pub struct StatusConsumer {
    state: Arc<AppState>,
}

impl StatusConsumer {
    pub fn new(state: Arc<AppState>) -> Self {
        Self { state }
    }

    async fn parse_message(&self, content_str: &str) -> Result<StatusMessage> {
        match serde_json::from_str::<Message>(content_str) {
            Ok(Message::WorkerStatus { status }) => Ok(status),
            Ok(_) => Err(eyre!("Received unexpected message")),
            Err(e) => {
                error!("Error parsing message: {:?}", e);
                Err(e.into())
            }
        }
    }

    #[tracing::instrument(skip(self, status_message), fields(worker_name = %status_message.worker_name))]
    async fn process_message(&self, status_message: StatusMessage) -> Result<()> {
        info!("Handling status message");
        debug!("Handling status message: {:?}", status_message);

        match status_message.status {
            WorkerStatusDetails::Lifecycle(status) => {
                self.state
                    .repo
                    .worker
                    .update_worker_status(
                        &status_message.worker_name,
                        &status.worker_status,
                        status_message.timestamp,
                    )
                    .await?;

                // Create or remove worker topics based on worker status
                match status.worker_status {
                    WorkerStatus::Online => {
                        self.state
                            .repo
                            .topic
                            .upsert_worker_topics(&status_message.worker_name, status.worker_topics)
                            .await?
                    }
                    WorkerStatus::Offline => {
                        self.state
                            .repo
                            .topic
                            .remove_worker_topics(&status_message.worker_name)
                            .await?
                    }
                }
            }
            WorkerStatusDetails::Job(job_details) => {
                let job_id = job_details.as_ref().map(|j| j.job_id);
                self.state
                    .repo
                    .worker
                    .update_worker_job(status_message.worker_name, job_id, status_message.timestamp)
                    .await?;

                let sub_job_id = job_details.as_ref().map(|j| j.sub_job_id);
                if let Some(sub_job_id) = sub_job_id {
                    self.state
                        .repo
                        .sub_job
                        .update_sub_job_status(&sub_job_id, SubJobStatus::Processing)
                        .await?;
                }
            }
            WorkerStatusDetails::Heartbeat => {
                self.state
                    .repo
                    .worker
                    .update_worker_heartbeat(status_message.worker_name, status_message.timestamp)
                    .await?;
            }
        }

        Ok(())
    }

    async fn run(&self, content: Vec<u8>) -> Result<()> {
        let content_str = String::from_utf8(content)?;

        debug!("Received message: {}", content_str);

        let status_message = self.parse_message(&content_str).await?;

        self.process_message(status_message).await?;

        Ok(())
    }
}

#[async_trait]
impl AsyncConsumer for StatusConsumer {
    async fn consume(
        &mut self,
        channel: &Channel,
        deliver: Deliver,
        _basic_properties: BasicProperties,
        content: Vec<u8>,
    ) {
        match self.run(content).await {
            Ok(_) => {
                debug!("Processed message successfully");
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
