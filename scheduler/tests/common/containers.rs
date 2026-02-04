// scheduler/tests/common/containers.rs
//
// Container lifecycle management for integration tests.
// Uses the same pattern as url_finder: AsyncRunner with owned containers.
// Containers are automatically cleaned up when all Arc references are dropped.

use std::sync::{Arc, LazyLock, Weak};
use std::time::Duration;
use testcontainers::{
    core::{IntoContainerPort, WaitFor},
    runners::AsyncRunner,
    ContainerAsync, GenericImage, ImageExt,
};
use tokio::sync::Mutex;

pub struct ContainerState {
    // Owned containers - will be dropped when ContainerState is dropped
    _postgres: ContainerAsync<GenericImage>,
    _rabbitmq: ContainerAsync<GenericImage>,
    pub postgres_port: u16,
    pub rabbitmq_port: u16,
}

static CONTAINERS: LazyLock<Mutex<Weak<ContainerState>>> =
    LazyLock::new(|| Mutex::new(Weak::new()));

pub async fn get_or_create_containers() -> Arc<ContainerState> {
    let mut weak_lock = CONTAINERS.lock().await;

    // Try to reuse existing containers
    if let Some(arc) = weak_lock.upgrade() {
        return arc;
    }

    // Start Postgres container using AsyncRunner
    let postgres = GenericImage::new("postgres", "16-alpine")
        .with_exposed_port(5432.tcp())
        .with_wait_for(WaitFor::message_on_stderr(
            "database system is ready to accept connections",
        ))
        .with_env_var("POSTGRES_PASSWORD", "postgres")
        .with_env_var("POSTGRES_USER", "postgres")
        .with_env_var("POSTGRES_DB", "postgres")
        .with_startup_timeout(Duration::from_secs(120))
        .start()
        .await
        .expect("Failed to start Postgres container");

    let postgres_port = postgres
        .get_host_port_ipv4(5432)
        .await
        .expect("Failed to get Postgres port");

    // Start RabbitMQ container using AsyncRunner
    let rabbitmq = GenericImage::new("rabbitmq", "3.13-alpine")
        .with_exposed_port(5672.tcp())
        .with_wait_for(WaitFor::seconds(10))
        .with_startup_timeout(Duration::from_secs(120))
        .start()
        .await
        .expect("Failed to start RabbitMQ container");

    let rabbitmq_port = rabbitmq
        .get_host_port_ipv4(5672)
        .await
        .expect("Failed to get RabbitMQ port");

    let state = Arc::new(ContainerState {
        _postgres: postgres,
        _rabbitmq: rabbitmq,
        postgres_port,
        rabbitmq_port,
    });

    *weak_lock = Arc::downgrade(&state);
    state
}
