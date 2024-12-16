use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

use amqprs::{
    channel::{BasicPublishArguments, Channel, ExchangeDeclareArguments},
    BasicProperties,
};
use color_eyre::Result;
use tokio::{sync::Mutex, time::sleep};
use tracing::{debug, error, info, instrument};

use crate::{config::PublisherConfig, messages::Message, ConnectionManager};

pub fn start_publisher(
    config: PublisherConfig,
    connection_manager: Arc<ConnectionManager>,
) -> Arc<Publisher> {
    let publisher = Arc::new(Publisher::new(config, connection_manager));
    let publisher_clone = publisher.clone();
    tokio::spawn(async move {
        publisher_clone.run().await;
    });

    publisher
}

pub struct Publisher {
    pub config: PublisherConfig,
    connection_manager: Arc<ConnectionManager>,
    channel: Arc<Mutex<Option<Channel>>>,
    running: AtomicBool,
}

impl Publisher {
    pub fn new(config: PublisherConfig, connection_manager: Arc<ConnectionManager>) -> Self {
        Self {
            config,
            connection_manager,
            channel: Arc::new(Mutex::new(None)),
            running: AtomicBool::new(true),
        }
    }

    #[instrument(skip(self), fields(routing_key = ?self.config.routing_key, exchange_name = %self.config.exchange_config.exchange_name))]
    pub async fn ensure_channel(&self) -> Result<Channel, Box<dyn std::error::Error>> {
        let mut chan_lock = self.channel.lock().await;

        if let Some(channel) = chan_lock.as_ref() {
            if channel.is_open() && channel.is_connection_open() {
                debug!("Channel is open");
                return Ok(channel.clone());
            }
            error!("Channel is closed");
        }

        debug!("No channel, creating new one");

        let connection = self
            .connection_manager
            .get_connection()
            .await
            .ok_or("No connection")?;

        info!("Opening channel for publisher");
        let channel = connection.open_channel(None).await.map_err(|e| {
            error!("Failed to open channel: {:?}", e);
            e
        })?;

        channel
            .exchange_declare(
                ExchangeDeclareArguments::new(
                    self.config.exchange_config.exchange_name,
                    self.config.exchange_config.exchange_type,
                )
                .passive(false)
                .durable(self.config.exchange_config.durable)
                .finish(),
            )
            .await
            .map_err(|e| {
                error!("Failed to declare exchange: {:?}", e);
                e
            })?;

        *chan_lock = Some(channel.clone());
        info!("New channel for publisher established");

        Ok(channel)
    }

    #[instrument(skip(self), fields(exchange_name = %self.config.exchange_config.exchange_name))]
    pub async fn run(&self) {
        debug!("Publisher ensure_channel loop start");

        while self.running.load(Ordering::SeqCst) {
            match self.ensure_channel().await {
                Ok(_) => debug!("Channel for publisher is ready"),
                Err(e) => error!("ensure_channel error for publisher: {:?}", e),
            };

            sleep(Duration::from_secs(2)).await;
        }
    }

    #[instrument(skip(self, message), fields(exchange_name = %self.config.exchange_config.exchange_name))]
    pub async fn publish(
        &self,
        message: &Message,
        routing_key: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        debug!("Publishing message: {:?}", message);

        let channel = self.ensure_channel().await.inspect_err(|e| {
            error!("ensure_channel error: {:?}", e);
        })?;

        let serialized_message = serde_json::to_vec(message)?;
        let args =
            BasicPublishArguments::new(self.config.exchange_config.exchange_name, routing_key);

        channel
            .basic_publish(BasicProperties::default(), serialized_message, args)
            .await
            .map_err(|e| {
                error!("Failed to publish message: {:?}", e);
                e
            })?;

        Ok(())
    }

    #[instrument(skip(self), fields(exchange_name = %self.config.exchange_config.exchange_name))]
    pub async fn close_channel(&self) {
        let mut chan_lock = self.channel.lock().await;

        self.running.store(false, Ordering::SeqCst);

        if let Some(channel) = chan_lock.take() {
            match channel.close().await {
                Ok(_) => info!("Channel closed"),
                Err(e) => error!("Failed to close channel: {:?}", e),
            }
        }
    }
}
