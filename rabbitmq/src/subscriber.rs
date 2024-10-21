use std::sync::Arc;

use amqprs::{
    channel::{
        BasicConsumeArguments, Channel, ExchangeDeclareArguments, QueueBindArguments,
        QueueDeclareArguments,
    },
    connection::Connection,
    consumer::AsyncConsumer,
};
use color_eyre::Result;
use tokio::sync::Mutex;

use crate::config::SubscriberConfig;

pub struct Subscriber {
    channel: Option<Channel>,
    config: SubscriberConfig,
}

impl Subscriber {
    pub fn new(config: SubscriberConfig) -> Self {
        Self {
            channel: None,
            config,
        }
    }

    pub fn set_queue_name(&mut self, queue_name: &'static str) {
        self.config.queue_name = Some(queue_name);
    }

    pub fn set_routing_keys(&mut self, routing_keys: Vec<&'static str>) {
        self.config.routing_keys = Some(routing_keys);
    }

    pub async fn setup(&mut self, connection: Arc<Mutex<Connection>>) -> Result<()> {
        let connection = connection.lock().await;
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
            QueueDeclareArguments::durable_client_named(self.config.queue_name.unwrap())
        } else {
            QueueDeclareArguments::transient_autodelete(self.config.queue_name.unwrap())
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

        self.channel = Some(channel);
        Ok(())
    }

    pub async fn subscribe<C>(&self, consumer: C) -> Result<(), Box<dyn std::error::Error>>
    where
        C: AsyncConsumer + Send + Sync + 'static,
    {
        let queue_name = self.config.queue_name.expect("Queue name must be set");
        let args = BasicConsumeArguments::new(queue_name, "");

        self.channel
            .as_ref()
            .ok_or("Channel not initialized")?
            .basic_consume(consumer, args)
            .await?;

        Ok(())
    }

    pub async fn close(&mut self) -> Result<()> {
        if let Some(channel) = self.channel.take() {
            channel.close().await?;
        }
        self.channel = None;
        Ok(())
    }
}
