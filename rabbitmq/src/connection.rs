use std::{
    env,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

use amqprs::{
    connection::{Connection, OpenConnectionArguments},
    tls::TlsAdaptor,
};
use color_eyre::Result;
use tokio::{
    sync::{Mutex, Notify},
    time::sleep,
};
use tracing::{debug, error, info};
use url::Url;

pub async fn start_connection_manager() -> Arc<ConnectionManager> {
    let connection_manager = Arc::new(ConnectionManager::new());
    let connection_manager_clone = connection_manager.clone();
    tokio::spawn(async move {
        connection_manager_clone.run().await;
    });
    connection_manager.wait_for_connection().await;

    connection_manager
}

pub struct ConnectionManager {
    connection: Arc<Mutex<Option<Connection>>>,
    notify_connected: Arc<Notify>,
    running: AtomicBool,
}

impl ConnectionManager {
    pub fn new() -> Self {
        Self {
            connection: Arc::new(Mutex::new(None)),
            notify_connected: Arc::new(Notify::new()),
            running: AtomicBool::new(true),
        }
    }

    pub async fn wait_for_connection(&self) {
        self.notify_connected.notified().await;
    }

    async fn connect(
        &self,
        addr: &str,
        port: u16,
        username: &str,
        password: &str,
        is_ssl: bool,
    ) -> Result<()> {
        {
            let conn_lock = self.connection.lock().await;

            if conn_lock.as_ref().is_some() && conn_lock.as_ref().unwrap().is_open() {
                debug!("Connection is already open");
                return Ok(());
            }
        }
        error!("Connection is closed, reconnecting...");

        let mut args = OpenConnectionArguments::new(addr, port, username, password);
        if is_ssl {
            args.tls_adaptor(
                TlsAdaptor::without_client_auth(None, addr.to_string())
                    .expect("Failed to create TLS adaptor"),
            );
        }

        let connection_future = Connection::open(&args);

        match connection_future.await {
            Ok(connection) => {
                info!("Connected to rabbitmq");
                {
                    let mut conn_lock = self.connection.lock().await;
                    *conn_lock = Some(connection);
                }
                debug!("Notify waiters");
                self.notify_connected.notify_waiters();
                debug!("Notified waiters");
            }
            Err(e) => error!("Connection failed: {:?}. Retrying...", e),
        }

        Ok(())
    }

    pub async fn run(&self) {
        let endpoint = env::var("RABBITMQ_ENDPOINT").expect("RABBITMQ_ENDPOINT must be set");
        let parsed_url = Url::parse(&endpoint).expect("Invalid URL format for RABBITMQ_ENDPOINT");

        let addr = parsed_url
            .host_str()
            .expect("RABBITMQ_ENDPOINT must contain a host");

        let is_ssl = match parsed_url.scheme() {
            "amqp" => false,
            "http" => false,
            "amqps" => true,
            "amqps+ssl" => true,
            "amqps+tls" => true,
            "https" => true,
            _ => panic!("Invalid scheme for RABBITMQ_ENDPOINT"),
        };

        let port = parsed_url.port().unwrap_or(5672);

        let username = env::var("RABBITMQ_USERNAME").expect("RABBITMQ_USERNAME must be set");
        let password = env::var("RABBITMQ_PASSWORD").expect("RABBITMQ_PASSWORD must be set");

        debug!("Running rabbitmq connection loop");

        while self.running.load(Ordering::SeqCst) {
            match self.connect(addr, port, &username, &password, is_ssl).await {
                Ok(_) => {}
                Err(e) => error!("Connection failed: {:?}, retrying...", e),
            }

            sleep(Duration::from_secs(5)).await;
        }
    }

    pub async fn get_connection(&self) -> Option<Connection> {
        let conn_lock = self.connection.lock().await;

        conn_lock.clone()
    }

    pub async fn close_connection(&self) {
        let mut conn_lock = self.connection.lock().await;

        self.running.store(false, Ordering::SeqCst);

        if let Some(connection) = conn_lock.as_ref() {
            match connection.clone().close().await {
                Ok(_) => info!("RabbitMQ connection closed"),
                Err(e) => error!("RabbitMQ failed to close connection: {:?}", e),
            }
        }

        *conn_lock = None;
    }
}

impl Default for ConnectionManager {
    fn default() -> Self {
        Self::new()
    }
}
