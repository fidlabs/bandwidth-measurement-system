use std::{env, sync::Arc};

use amqprs::{
    connection::{Connection, OpenConnectionArguments},
    tls::TlsAdaptor,
};
use color_eyre::Result;
use tokio::sync::Mutex;
use url::Url;

pub async fn get_connection() -> Result<Arc<Mutex<Connection>>> {
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

    // Open connection
    let connection = if is_ssl {
        Connection::open(
            OpenConnectionArguments::new(addr, port, &username, &password)
                .tls_adaptor(TlsAdaptor::without_client_auth(None, addr.to_string()).unwrap()),
        )
        .await?
    } else {
        Connection::open(&OpenConnectionArguments::new(
            addr, port, &username, &password,
        ))
        .await?
    };

    Ok(Arc::new(Mutex::new(connection)))
}
