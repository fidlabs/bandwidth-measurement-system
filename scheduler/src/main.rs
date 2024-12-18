use std::{error::Error, sync::Arc};

use api::api_doc::ApiDoc;
use axum::Router;
use background::{
    service_descaler::service_descaler_handler, sub_job_handler::sub_job_handler,
    worker_online_check::process_worker_online_check,
};
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
};
use tower::ServiceBuilder;
use tower_http::trace::TraceLayer;
use tracing::info;
use tracing_subscriber::EnvFilter;
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

mod api;
mod background;
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
        .inspect_err(|_| info!("Failed to read .env file, ignoring."))
        .ok();

    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or(EnvFilter::new(CONFIG.log_level.clone())),
        )
        .init();

    info!("Connecting to PostgreSQL...");
    let pool = PgPool::connect(&CONFIG.db_url).await?;
    info!("Successfully connected to PostgreSQL, Running database migrations...");
    MIGRATOR.run(&pool).await?;
    info!("Successfully ran database migrations");

    info!("Connecting to RabbitMQ...");
    // Initialize RabbitMQ connection
    let rabbit_connection_manager = start_connection_manager().await;

    // Initialize job queue publisher
    let job_queue_publisher = start_publisher(
        get_publisher_config(PublisherType::JobPublisher),
        rabbit_connection_manager.clone(),
    );

    // Initialize repositories
    let repo = Arc::new(Repositories::new(pool.clone()));

    // Initialize service scaler registry
    let service_scaler_registry = Arc::new(ServiceScalerRegistry::new());

    // Initialize app state
    let app_state = Arc::new(AppState::new(repo.clone(), service_scaler_registry.clone()));

    // Backgroud processes
    tokio::spawn(sub_job_handler(
        repo.clone(),
        job_queue_publisher.clone(),
        service_scaler_registry.clone(),
    ));
    tokio::spawn(service_descaler_handler(
        repo.clone(),
        service_scaler_registry.clone(),
    ));
    tokio::spawn(process_worker_online_check(repo.clone()));

    // Start the data queue subscriber
    let data_queue_subscriber = start_subscriber(
        get_subscriber_config(SubscriberType::ResultSubscriber),
        rabbit_connection_manager.clone(),
        DataConsumer::new(Arc::clone(&app_state)),
        None,
        None,
    );
    // Start the status queue subscriber
    let status_queue_subscriber = start_subscriber(
        get_subscriber_config(SubscriberType::StatusSubscriber),
        rabbit_connection_manager.clone(),
        StatusConsumer::new(Arc::clone(&app_state)),
        None,
        None,
    );

    let app = Router::new()
        .merge(routes::create_routes())
        .merge(SwaggerUi::new("/swagger-ui").url("/api-doc/openapi.json", ApiDoc::openapi()))
        .layer(ServiceBuilder::new().layer(TraceLayer::new_for_http()))
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
    job_queue_publisher.close_channel().await;
    data_queue_subscriber.close_channel().await;
    status_queue_subscriber.close_channel().await;
    rabbit_connection_manager.close_connection().await;

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
