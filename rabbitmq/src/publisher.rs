use std::sync::Arc;

use amqprs::{
    channel::{BasicPublishArguments, Channel, ExchangeDeclareArguments},
    connection::Connection,
    BasicProperties,
};
use color_eyre::Result;
use tokio::sync::Mutex;

use crate::{config::PublisherConfig, messages::Message};

#[derive(Clone)]
pub struct Publisher {
    pub channel: Option<Channel>,
    pub config: PublisherConfig,
}

impl Publisher {
    pub fn new(config: PublisherConfig) -> Self {
        Self {
            channel: None,
            config,
        }
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
                .finish(),
            )
            .await?;

        self.channel = Some(channel);
        Ok(())
    }

    pub async fn publish(
        &self,
        message: &Message,
        routing_key: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let serialized_message = serde_json::to_vec(message)?;
        let args =
            BasicPublishArguments::new(self.config.exchange_config.exchange_name, routing_key);

        self.channel
            .as_ref()
            .ok_or("Channel not initialized")?
            .basic_publish(BasicProperties::default(), serialized_message, args)
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
