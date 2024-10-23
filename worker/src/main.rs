use std::error::Error;

use color_eyre::Result;
use config::CONFIG;
use queue::{job_consumer::JobConsumer, status_sender::StatusSender};
use rabbitmq::*;
use tokio::{
    signal::unix::{signal, SignalKind},
    time::{interval, Duration},
};
use tracing::{error, info};
use tracing_subscriber::EnvFilter;

mod config;
mod handlers;
mod queue;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    info!("Worker is starting...");

    // Initialize color_eyre panic and error handlers
    color_eyre::install()?;

    // Load .env
    dotenvy::dotenv()
        .inspect_err(|_| eprintln!("Failed to read .env file, ignoring."))
        .ok();

    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or(EnvFilter::new(CONFIG.log_level.clone())),
        )
        .init();

    info!(
        "Worker started, name: {} topics: {:?}",
        CONFIG.worker_name.to_string(),
        CONFIG.worker_topics,
    );

    // Initialize RabbitMQ connection
    let rabbit_connection = rabbitmq::get_connection().await?;
    info!("Successfully connected to RabbitMQ");

    let mut job_queue = Subscriber::new(get_subscriber_config(SubscriberType::JobSubscriber));
    job_queue.set_queue_name(CONFIG.worker_name.as_str());
    job_queue.set_routing_keys(
        CONFIG
            .worker_topics
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<&str>>(),
    );
    job_queue.setup(rabbit_connection.clone()).await?;
    info!("Successfully set up job queue");

    let mut data_queue = Publisher::new(get_publisher_config(PublisherType::ResultPublisher));
    data_queue.setup(rabbit_connection.clone()).await?;
    info!("Successfully set up data queue");

    let mut status_queue = Publisher::new(get_publisher_config(PublisherType::StatusPublisher));
    status_queue.setup(rabbit_connection.clone()).await?;
    info!("Successfully set up status queue");
    let status_sender = StatusSender::new(status_queue.clone());

    status_sender
        .send_lifecycle_status(WorkerStatus::Online)
        .await?;

    // Spawn the background task to send heartbeat status
    tokio::spawn(send_heartbeat_status(status_sender.clone()));

    let consumer = JobConsumer::new(data_queue.clone(), status_sender.clone());
    job_queue.subscribe(consumer).await?;
    info!("Successfully started job queue consumer");

    let mut sigint = signal(SignalKind::interrupt())?;
    let mut sigterm = signal(SignalKind::terminate())?;

    info!("Worker is ready. Waiting for shutdown signal...");

    tokio::select! {
        _ = sigint.recv() => {
            info!("Received SIGINT signal, shutting down...");
        }
        _ = sigterm.recv() => {
            info!("Received SIGTERM signal, shutting down...");
        }
    }
    info!("Worker is shutting down...");

    status_sender
        .send_lifecycle_status(WorkerStatus::Offline)
        .await?;
    info!("Worker sent offline status");

    // TODO: do not accept new jobs and wait for execution of existing ones
    // TODO: maybe lookup tokio::sync::Notify for this

    // Close the connection gracefully
    job_queue.close().await?;
    data_queue.close().await?;
    status_queue.close().await?;
    rabbit_connection.lock().await.clone().close().await?;

    info!("Worker shut down gracefully");

    Ok(())
}

/// Sends heartbeat status to scheduler every interval
async fn send_heartbeat_status(status_sender: StatusSender) {
    let interval_secs: u64 = CONFIG.heartbeat_interval_sec;

    let mut interval = interval(Duration::from_secs(interval_secs));

    loop {
        interval.tick().await;
        if let Err(e) = status_sender.send_heartbeat_status().await {
            error!("Error sending heartbeat status: {}", e);
        }
    }
}
