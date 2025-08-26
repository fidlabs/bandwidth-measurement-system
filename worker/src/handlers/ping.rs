use std::net::{IpAddr, ToSocketAddrs};

use chrono::{Duration, Utc};
use color_eyre::Result;
use rabbitmq::{JobMessage, PingError, PingResult};
use rand::random;
use surge_ping::{Client, Config, PingIdentifier, PingSequence, ICMP};
use tracing::{debug, error, info};
use url::Url;
use uuid::Uuid;

#[tracing::instrument(skip(payload))]
pub async fn process(job_id: Uuid, payload: JobMessage) -> Result<PingResult, PingError> {
    info!("Processing PING job");

    // Calculate deadline
    let loop_deadline = payload.download_start_time - Duration::seconds(2);

    debug!("now: {} loop_deadline: {}", Utc::now(), loop_deadline);

    // Parse the URL and extract the host
    let url = Url::parse(&payload.url).map_err(|e| PingError {
        error: format!("UrlParseError: {e}"),
    })?;
    let host = url.host_str().ok_or(PingError {
        error: "Failed to extract host from URL".to_string(),
    })?;

    // Resolve the host to an IP address
    let ip_address: IpAddr = (host, 0)
        .to_socket_addrs()
        .map_err(|_| PingError {
            error: "Failed to extract IP address from socket addr".to_string(),
        })?
        .map(|socket_addr| socket_addr.ip())
        .collect::<Vec<IpAddr>>()
        .first()
        .cloned() // Convert Option<&T> to Option<T>
        .ok_or(PingError {
            error: "Failed to extract IP address from socket addr".to_string(),
        })?;

    let config = match ip_address {
        IpAddr::V4(_) => Config::default(),
        IpAddr::V6(_) => Config::builder().kind(ICMP::V6).build(),
    };
    let client = Client::new(&config).map_err(|e| PingError {
        error: format!("SurgePingClientError: {e}"),
    })?;
    let mut pinger = client.pinger(ip_address, PingIdentifier(random())).await;

    let mut latencies: Vec<f64> = Vec::new();
    let seq_max = 10;
    let packets_threshold = seq_max / 2;

    for seq in 0..seq_max {
        // Check deadline
        if Utc::now() >= loop_deadline {
            info!("Loop deadline reached, aborting the loop");
            break;
        }

        let (_, duration) = match pinger.ping(PingSequence(seq), &[6, 6, 6]).await {
            Ok((packet, duration)) => (packet, duration),
            Err(e) => {
                error!("Failed to ping host: {}", e);
                continue;
            }
        };
        latencies.push(duration.as_secs_f64());
    }

    // Check if we have at least half of the packets
    if latencies.len() < packets_threshold.into() {
        return Err(PingError {
            error: "Too many packets lost".to_string(),
        });
    }

    // Calculate the average, min, and max latencies
    let avg_latency: f64 = latencies.iter().sum::<f64>() / latencies.len() as f64;
    let min_latency: f64 = *latencies
        .iter()
        .min_by(|a, b| a.partial_cmp(b).unwrap())
        .unwrap();
    let max_latency: f64 = *latencies
        .iter()
        .max_by(|a, b| a.partial_cmp(b).unwrap())
        .unwrap();

    debug!("Latency Statistics:");
    debug!("Average: {:.2} ms", avg_latency);
    debug!("Min: {:.2} ms", min_latency);
    debug!("Max: {:.2} ms", max_latency);

    Ok(PingResult {
        avg: avg_latency,
        min: min_latency,
        max: max_latency,
        ip_address: ip_address.to_string(),
    })
}
