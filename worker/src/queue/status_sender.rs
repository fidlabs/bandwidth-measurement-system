use std::sync::Arc;

use chrono::Utc;
use rabbitmq::{
    Message, Publisher, StatusMessage, WorkerDetails, WorkerStatus, WorkerStatusDetails,
    WorkerStatusJobDetails,
};

use crate::CONFIG;

#[derive(Clone)]
pub struct StatusSender {
    status_queue: Arc<Publisher>,
}

impl StatusSender {
    pub fn new(status_queue: Arc<Publisher>) -> Self {
        StatusSender { status_queue }
    }

    pub async fn send_lifecycle_status(
        &self,
        status: WorkerStatus,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let message = Message::WorkerStatus {
            status: StatusMessage {
                status: WorkerStatusDetails::Lifecycle(WorkerDetails {
                    worker_topics: CONFIG.worker_topics.clone(),
                    worker_status: status,
                }),
                timestamp: Utc::now(),
                worker_name: CONFIG.worker_name.to_string(),
            },
        };

        self.send_status(message).await?;

        Ok(())
    }

    pub async fn send_job_status(
        &self,
        job_details: Option<WorkerStatusJobDetails>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let message = Message::WorkerStatus {
            status: StatusMessage {
                status: WorkerStatusDetails::Job(job_details),
                timestamp: Utc::now(),
                worker_name: CONFIG.worker_name.to_string(),
            },
        };

        self.send_status(message).await?;

        Ok(())
    }

    pub async fn send_heartbeat_status(&self) -> Result<(), Box<dyn std::error::Error>> {
        let message = Message::WorkerStatus {
            status: StatusMessage {
                status: WorkerStatusDetails::Heartbeat,
                timestamp: Utc::now(),
                worker_name: CONFIG.worker_name.to_string(),
            },
        };

        self.send_status(message).await?;

        Ok(())
    }

    async fn send_status(&self, message: Message) -> Result<(), Box<dyn std::error::Error>> {
        self.status_queue
            .publish(
                &message,
                self.status_queue
                    .config
                    .routing_key
                    .ok_or("Missing status queue routing key")?,
            )
            .await?;

        Ok(())
    }
}
