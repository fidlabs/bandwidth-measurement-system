use std::{sync::Arc, time::Duration};

use amqprs::{
    channel::{
        BasicConsumeArguments, Channel, ExchangeDeclareArguments, QueueBindArguments,
        QueueDeclareArguments,
    },
    consumer::AsyncConsumer,
};
use color_eyre::{eyre::eyre, Result};
use tokio::{sync::Mutex, time::sleep};
use tracing::{debug, error, info, instrument};

use crate::{config::SubscriberConfig, ConnectionManager};

/// spawn a subscriber task
pub fn start_subscriber<C>(
    config: SubscriberConfig,
    connection_manager: Arc<ConnectionManager>,
    consumer: C,
    queue_name: Option<&'static str>,
    routing_keys: Option<Vec<&'static str>>,
) -> Arc<Subscriber>
where
    C: AsyncConsumer + Send + Sync + Clone + 'static,
{
    let mut subscriber = Subscriber::new(config, connection_manager);
    if let Some(queue_name) = queue_name {
        subscriber.set_queue_name(queue_name);
    }
    if let Some(routing_keys) = routing_keys {
        subscriber.set_routing_keys(routing_keys);
    }
    let subscriber = Arc::new(subscriber);
    let subscriber_clone = subscriber.clone();

    tokio::spawn(async move {
        subscriber_clone.run(consumer).await;
    });

    subscriber
}

pub struct Subscriber {
    config: SubscriberConfig,
    connection_manager: Arc<ConnectionManager>,
    channel: Arc<Mutex<Option<Channel>>>,
}

impl Subscriber {
    pub fn new(config: SubscriberConfig, connection_manager: Arc<ConnectionManager>) -> Self {
        Self {
            config,
            connection_manager,
            channel: Arc::new(Mutex::new(None)),
        }
    }

    pub fn set_queue_name(&mut self, queue_name: &'static str) {
        self.config.queue_name = Some(queue_name);
    }

    pub fn set_routing_keys(&mut self, routing_keys: Vec<&'static str>) {
        self.config.routing_keys = Some(routing_keys);
    }

    #[instrument(skip(self, consumer), fields(exchange_name = %self.config.exchange_config.exchange_name, queue_name = ?self.config.queue_name))]
    async fn setup<C>(&self, consumer: C) -> Result<()>
    where
        C: AsyncConsumer + Send + Sync + Clone + 'static,
    {
        let connection = self
            .connection_manager
            .get_connection()
            .await
            .ok_or(eyre!("no connection"))?;

        let mut chan_lock = self.channel.lock().await;
        if chan_lock.is_some() && chan_lock.as_ref().unwrap().is_open() {
            return Ok(());
        }

        let channel = connection.open_channel(None).await?;
        channel
            .exchange_declare(
                ExchangeDeclareArguments::new(
                    self.config.exchange_config.exchange_name,
                    self.config.exchange_config.exchange_type,
                )
                .passive(false)
                .durable(self.config.exchange_config.durable)
                .auto_delete(!self.config.exchange_config.durable)
                .finish(),
            )
            .await?;

        let queue_name = self.config.queue_name.expect("Queue name must be set");
        let routing_keys = self
            .config
            .routing_keys
            .as_ref()
            .expect("Routing keys must be set");

        let queue_args = if self.config.durable {
            QueueDeclareArguments::durable_client_named(queue_name)
        } else {
            QueueDeclareArguments::transient_autodelete(queue_name)
        };
        channel.queue_declare(queue_args).await?;

        for routing_key in routing_keys {
            channel
                .queue_bind(QueueBindArguments::new(
                    queue_name,
                    self.config.exchange_config.exchange_name,
                    routing_key,
                ))
                .await?;
        }

        *chan_lock = Some(channel.clone());

        channel
            .basic_consume(consumer.clone(), BasicConsumeArguments::new(queue_name, ""))
            .await?;

        info!("Successfully started queue consumer");

        Ok(())
    }

    #[instrument(skip(self, consumer), fields(exchange_name = %self.config.exchange_config.exchange_name, queue_name = ?self.config.queue_name))]
    pub async fn run<C>(&self, consumer: C)
    where
        C: AsyncConsumer + Send + Sync + Clone + 'static,
    {
        loop {
            match self.setup(consumer.clone()).await {
                Ok(_) => debug!("Subscriber channel established"),
                Err(e) => error!("failed to set up subscriber: {:?}", e),
            }

            sleep(Duration::from_secs(5)).await;
        }
    }

    #[instrument(skip(self), fields(exchange_name = %self.config.exchange_config.exchange_name, queue_name = ?self.config.queue_name))]
    pub async fn close_channel(&self) {
        let mut chan_lock = self.channel.lock().await;
        if let Some(channel) = chan_lock.take() {
            match channel.close().await {
                Ok(_) => info!("Channel closed"),
                Err(e) => error!("Failed to close channel: {:?}", e),
            }
        }
    }
}
