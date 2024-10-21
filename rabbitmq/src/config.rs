#[derive(Clone)]
pub struct ExchangeConfig {
    pub exchange_name: &'static str,
    pub exchange_type: &'static str,
    pub durable: bool,
}

#[derive(Clone)]
pub struct PublisherConfig {
    pub exchange_config: ExchangeConfig,
    pub routing_key: Option<&'static str>,
}

#[derive(Clone)]
pub struct SubscriberConfig {
    pub exchange_config: ExchangeConfig,
    pub queue_name: Option<&'static str>,
    pub routing_keys: Option<Vec<&'static str>>,
    pub durable: bool,
}

pub enum PublisherType {
    JobPublisher,
    ResultPublisher,
    StatusPublisher,
}

pub enum SubscriberType {
    JobSubscriber,
    ResultSubscriber,
    StatusSubscriber,
}

pub enum ExchangeType {
    JobExchange,
    ResultExchange,
    StatusExchange,
}

pub fn get_exchange_config(exchange_type: ExchangeType) -> ExchangeConfig {
    match exchange_type {
        ExchangeType::JobExchange => ExchangeConfig {
            exchange_name: "job_exchange",
            exchange_type: "topic",
            durable: true,
        },
        ExchangeType::ResultExchange => ExchangeConfig {
            exchange_name: "result_exchange",
            exchange_type: "direct",
            durable: true,
        },
        ExchangeType::StatusExchange => ExchangeConfig {
            exchange_name: "status_exchange",
            exchange_type: "direct",
            durable: true,
        },
    }
}

pub fn get_publisher_config(pub_type: PublisherType) -> PublisherConfig {
    match pub_type {
        PublisherType::JobPublisher => PublisherConfig {
            exchange_config: get_exchange_config(ExchangeType::JobExchange),
            routing_key: None,
        },
        PublisherType::ResultPublisher => PublisherConfig {
            exchange_config: get_exchange_config(ExchangeType::ResultExchange),
            routing_key: Some("worker_result"),
        },
        PublisherType::StatusPublisher => PublisherConfig {
            exchange_config: get_exchange_config(ExchangeType::StatusExchange),
            routing_key: Some("worker_status"),
        },
    }
}

pub fn get_subscriber_config(sub_type: SubscriberType) -> SubscriberConfig {
    match sub_type {
        SubscriberType::JobSubscriber => SubscriberConfig {
            exchange_config: get_exchange_config(ExchangeType::JobExchange),
            queue_name: None,
            routing_keys: None,
            durable: false,
        },
        SubscriberType::ResultSubscriber => SubscriberConfig {
            exchange_config: get_exchange_config(ExchangeType::ResultExchange),
            queue_name: Some("result_queue"),
            routing_keys: Some(vec!["worker_result"]),
            durable: true,
        },
        SubscriberType::StatusSubscriber => SubscriberConfig {
            exchange_config: get_exchange_config(ExchangeType::StatusExchange),
            queue_name: Some("status_queue"),
            routing_keys: Some(vec!["worker_status"]),
            durable: true,
        },
    }
}
