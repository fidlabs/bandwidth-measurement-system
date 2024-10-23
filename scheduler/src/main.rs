use std::{error::Error, sync::Arc};

use axum::Router;
use color_eyre::Result;
use config::CONFIG;
use queue::{data_consumer::DataConsumer, status_consumer::StatusConsumer};
use rabbitmq::*;
use repository::*;
use service_scaler::ServiceScalerRegistry;
use sqlx::{migrate::Migrator, PgPool};
use state::AppState;
use tokio::{
    net::TcpListener,
    signal::unix::{signal, SignalKind},
    sync::Mutex,
};
use tower::ServiceBuilder;
use tower_http::trace::TraceLayer;
use tracing::info;
use tracing_subscriber::EnvFilter;

mod api;
mod config;
mod queue;
mod repository;
mod routes;
mod service_scaler;
mod state;
mod types;

static MIGRATOR: Migrator = sqlx::migrate!("./src/migrations");

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    info!("Scheduler is starting...");

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

    let pool = PgPool::connect(&CONFIG.db_url).await?;
    MIGRATOR.run(&pool).await?;

    // Initialize RabbitMQ connection
    let rabbit_connection = rabbitmq::get_connection().await?;
    info!("Successfully connected to RabbitMQ");

    let job_queue = Arc::new(Mutex::new(Publisher::new(get_publisher_config(
        PublisherType::JobPublisher,
    ))));
    job_queue
        .lock()
        .await
        .setup(rabbit_connection.clone())
        .await?;
    info!("Successfully set up job queue");

    // Initialize repositories
    let data_repo = Arc::new(DataRepository::new(pool.clone()));
    let worker_repo = Arc::new(WorkerRepository::new(pool.clone()));
    let job_repo = Arc::new(JobRepository::new(pool.clone()));
    let topic_repo = Arc::new(TopicRepository::new(pool.clone()));
    let sub_job_repo = Arc::new(SubJobRepository::new(pool.clone()));
    let service_repo = Arc::new(ServiceRepository::new(pool.clone()));

    // Initialize service scaler registry
    let service_scaler_registry = Arc::new(ServiceScalerRegistry::new());

    // Initialize app state
    let app_state = Arc::new(AppState {
        job_queue: job_queue.clone(),
        data_repo,
        worker_repo,
        job_repo,
        topic_repo,
        sub_job_repo,
        service_repo,
        service_scaler_registry,
    });

    let mut data_queue = Subscriber::new(get_subscriber_config(SubscriberType::ResultSubscriber));
    data_queue.setup(rabbit_connection.clone()).await?;
    info!("Successfully set up data queue");

    let data_consumer = DataConsumer::new(app_state.clone());
    data_queue.subscribe(data_consumer).await?;
    info!("Successfully started data queue consumer");

    let mut status_queue = Subscriber::new(get_subscriber_config(SubscriberType::StatusSubscriber));
    status_queue.setup(rabbit_connection.clone()).await?;
    let status_consumer = StatusConsumer::new(app_state.clone());
    status_queue.subscribe(status_consumer).await?;
    info!("Successfully started status queue consumer");

    let app = Router::new()
        .merge(routes::create_routes())
        .layer(
            ServiceBuilder::new().layer(TraceLayer::new_for_http()),
            // TODO: add something to authenticate requests
        )
        .with_state(app_state.clone());

    let server_addr = "0.0.0.0:3000".to_string();
    let listener = TcpListener::bind(&server_addr).await?;
    info!("Listening on http://{}", &server_addr);

    info!("Scheduler started successfully, waiting for requests...");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    // TODO: do not accept new jobs and wait for execution of existing ones
    // TODO: maybe lookup tokio::sync::Notify for this

    // Close the connection gracefully
    job_queue.lock().await.clone().close().await?;
    data_queue.close().await?;
    status_queue.close().await?;
    rabbit_connection.lock().await.clone().close().await?;

    info!("Scheduler shut down gracefully");

    Ok(())
}

async fn shutdown_signal() {
    let mut sigint = signal(SignalKind::interrupt()).expect("SIGINT signal handler failed");
    let mut sigterm = signal(SignalKind::terminate()).expect("SIGTERM signal handler failed");

    tokio::select! {
        _ = sigint.recv() => {
            info!("Received SIGINT signal, shutting down...");
        }
        _ = sigterm.recv() => {
            info!("Received SIGTERM signal, shutting down...");
        }
    }
}
