use chrono::{Duration, Utc};
use color_eyre::Result;
use rabbitmq::{HeadError, HeadResult, JobMessage};
use reqwest::Client;
use tokio::time::Instant;
use tracing::{debug, info};
use uuid::Uuid;

#[tracing::instrument(skip(payload))]
pub async fn process(job_id: Uuid, payload: JobMessage) -> Result<HeadResult, HeadError> {
    info!("Processing HEAD job");

    let client = Client::new();
    let num_requests = 10; // Number of times to send the HEAD request
    let mut latencies: Vec<f64> = Vec::with_capacity(num_requests);

    // Calculate deadline
    let loop_deadline = payload.download_start_time - Duration::seconds(2);

    debug!("now: {} loop_deadline: {}", Utc::now(), loop_deadline);

    for _ in 0..num_requests {
        // Check deadline
        if Utc::now() >= loop_deadline {
            info!("Loop deadline reached, aborting the loop");
            break;
        }

        let start_time = Instant::now(); // Start timing

        // Send a HEAD request to the URL
        let response = client
            .head(&payload.url)
            .send()
            .await
            .map_err(|e| HeadError {
                error: format!("RequestError: {}", e),
            })?;

        // Measure the elapsed time
        let elapsed = start_time.elapsed();
        let latency_ms = elapsed.as_secs_f64() * 1000.0; // Convert to milliseconds
        latencies.push(latency_ms);

        // Print the status code to verify the request
        debug!(
            "Response Status: {}, Latency: {}",
            response.status(),
            latency_ms
        );
    }

    if latencies.is_empty() {
        return Err(HeadError {
            error: "No successful requests".to_string(),
        });
    }

    // Calculate min, max, and average latencies
    let min_latency = latencies.iter().cloned().fold(f64::INFINITY, f64::min);
    let max_latency = latencies.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let avg_latency = latencies.iter().sum::<f64>() / latencies.len() as f64;

    debug!("Latency Statistics:");
    debug!("Average: {:.2} ms", avg_latency);
    debug!("Min: {:.2} ms", min_latency);
    debug!("Max: {:.2} ms", max_latency);

    info!("Finished processing HEAD job");

    Ok(HeadResult {
        min: min_latency,
        max: max_latency,
        avg: avg_latency,
    })
}
