use std::{error::Error, sync::Arc};

use color_eyre::Result;
use config::CONFIG;
use queue::{job_consumer::JobConsumer, status_sender::StatusSender};
use rabbitmq::*;
use tokio::{
    signal::unix::{signal, SignalKind},
    time::{interval, Duration},
};
use tracing::{debug, error, info};
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
    let rabbit_connection_manager = start_connection_manager().await;

    // Initialize data queue publisher
    let data_queue_publisher = start_publisher(
        get_publisher_config(PublisherType::ResultPublisher),
        rabbit_connection_manager.clone(),
    );

    // Initalize status queue publisher
    let status_queue_publisher = start_publisher(
        get_publisher_config(PublisherType::StatusPublisher),
        rabbit_connection_manager.clone(),
    );

    let status_sender = StatusSender::new(status_queue_publisher.clone());
    // Send online status to scheduler
    status_sender
        .send_lifecycle_status(WorkerStatus::Online)
        .await?;

    // Spawn the background task to send heartbeat status
    tokio::spawn(send_heartbeat_status(status_sender.clone()));

    let job_queue_subscriber = start_subscriber(
        get_subscriber_config(SubscriberType::JobSubscriber),
        rabbit_connection_manager.clone(),
        JobConsumer::new(data_queue_publisher.clone(), status_sender.clone()),
        Some(CONFIG.worker_name.as_str()),
        Some(
            CONFIG
                .worker_topics
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<&str>>(),
        ),
    );

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
    job_queue_subscriber.close_channel().await;
    data_queue_publisher.close_channel().await;
    status_queue_publisher.close_channel().await;
    rabbit_connection_manager.close_connection().await;

    info!("Worker shut down gracefully");

    Ok(())
}

/// Sends heartbeat status to scheduler every interval
async fn send_heartbeat_status(status_sender: StatusSender) {
    debug!("Starting heartbeat status sender...");
    let interval_secs: u64 = CONFIG.heartbeat_interval_sec;

    let mut interval = interval(Duration::from_secs(interval_secs));

    loop {
        interval.tick().await;
        debug!("Sending heartbeat status...");
        if let Err(e) = status_sender.send_heartbeat_status().await {
            error!("Error sending heartbeat status: {}", e);
        }
    }
}
