use bytes::Bytes;
use chrono::{DateTime, Duration, Utc};
use color_eyre::{eyre::bail, Result};
use rabbitmq::{AccumulatingBytes, DownloadError, DownloadResult, IntervalBytes, JobMessage};
use reqwest::{
    header::{ACCEPT, RANGE, USER_AGENT},
    Client, Response,
};
use tokio::time::{sleep, timeout};
use tracing::{debug, error, info};
use uuid::Uuid;

// Download deadline, job will succeed but won't work/download more than this duration
const MAX_DOWNLOAD_DURATION: Duration = Duration::seconds(60);

/// Prepare the HTTP request
fn prepare_request(url: &str, range_start: u64, range_end: u64) -> reqwest::RequestBuilder {
    const USER_AGENT_STR: &str = "curl/7.68.0";
    const ACCEPT_TYPE: &str = "*/*";

    Client::new()
        .get(url)
        .header(RANGE, format!("bytes={}-{}", range_start, range_end))
        .header(USER_AGENT, USER_AGENT_STR)
        .header(ACCEPT, ACCEPT_TYPE)
}

/// Calculates the next even second from the given time.
fn calculate_next_even_second(current: DateTime<Utc>) -> DateTime<Utc> {
    let millis = current.timestamp_millis() % 1000;
    let remaining_millis = 1000 - millis;

    current + Duration::milliseconds(remaining_millis)
}

/// Sleep until the start time of the job
async fn wait_for_start_time(payload: &JobMessage) -> Result<()> {
    let now = Utc::now();

    if payload.download_start_time < now {
        error!(
            "Start time is in the past, download_start_time: {}",
            payload.download_start_time
        );
        bail!(
            "Start time is in the past, now: {}, download_start_time: {}",
            now,
            payload.download_start_time
        );
    }

    let sleep_duration = payload.download_start_time - now;
    debug!("Sleeping for {:?}", sleep_duration);

    sleep(sleep_duration.to_std()?).await;

    debug!("Woke up after sleeping");

    Ok(())
}

async fn download_chunk(response: &mut Response) -> Result<Option<Bytes>, DownloadError> {
    match timeout(MAX_DOWNLOAD_DURATION.to_std().unwrap(), response.chunk()).await {
        Ok(Ok(chunk)) => Ok(chunk),
        Ok(Err(e)) => Err(DownloadError {
            error: format!("ChunkError: {}", e),
        }),
        _ => Ok(None),
    }
}

/// Benchmark the download speed of the given URL
#[tracing::instrument(skip(payload))]
pub async fn process(job_id: Uuid, payload: JobMessage) -> Result<DownloadResult, DownloadError> {
    info!("Processing Download job");

    let request = prepare_request(&payload.url, payload.start_range, payload.end_range);

    let job_start_time = Utc::now();
    let mut bytes: usize = 0;
    let mut total_bytes: usize = 0;
    let mut second_by_second_logs: Vec<(DateTime<Utc>, IntervalBytes, AccumulatingBytes)> =
        Vec::new();

    // Delay the download execution to sync the time on every worker
    wait_for_start_time(&payload)
        .await
        .map_err(|e| DownloadError {
            error: format!("TimeSyncError: {}", e),
        })?;

    let mut response = request.send().await.map_err(|e| DownloadError {
        error: format!("RequestError: {}", e),
    })?;

    if !response.status().is_success() {
        return Err(DownloadError {
            error: format!("RequestFailed: {}", response.status()),
        });
    }

    let time_to_first_byte_ms = (Utc::now() - job_start_time).num_milliseconds() as f64;
    debug!("Time to first byte: {} ms", time_to_first_byte_ms);

    // It seems that time to first byte can be quite long, so we need to adjust the start time for better download speed calculation
    let download_start_time = Utc::now();
    let mut next_log_time = calculate_next_even_second(download_start_time);

    debug!(
        "job_start_time: {}, download_start_time: {}, next_log_time: {}",
        job_start_time, download_start_time, next_log_time
    );

    while let Some(chunk) = download_chunk(&mut response).await? {
        let chunk_size = chunk.len();
        bytes += chunk_size;
        total_bytes += chunk_size;

        let current_time = Utc::now();
        let elapsed_time = current_time - download_start_time;
        if elapsed_time >= MAX_DOWNLOAD_DURATION {
            info!(
                "Reached maximum download duration of {:?}, stopping download",
                MAX_DOWNLOAD_DURATION
            );
            break;
        }
        // Save the data for each interval, close to each even second
        if current_time >= next_log_time {
            // Save the stats for the interval
            second_by_second_logs.push((
                current_time,
                IntervalBytes(bytes),
                AccumulatingBytes(total_bytes),
            ));
            debug!(
                "Time: {:?}, Bytes downloaded: {}",
                current_time, total_bytes
            );

            // Reset the interval byte counter
            bytes = 0;
            // Increment next log time to the next even second
            next_log_time = calculate_next_even_second(current_time);
            debug!(
                "Duration from current time {:?}",
                (next_log_time - current_time).num_milliseconds()
            );
        }
    }

    if total_bytes == 0 {
        return Err(DownloadError {
            error: "Downloaded 0 bytes".to_string(),
        });
    }

    let end_time = Utc::now();
    let elapsed_secs = (end_time - download_start_time).num_milliseconds() as f64 / 1000.0;
    // Convert to bits and then to kilo and mega bits per second
    let download_speed = (total_bytes as f64 * 8.0) / (elapsed_secs * 1024.0 * 1024.0);

    info!(
        "Downloaded {} bytes in {:.2} seconds ({:.2} Mbps, {:.2} MBps)",
        total_bytes,
        elapsed_secs,
        download_speed,
        download_speed / 8.0
    );

    Ok(DownloadResult {
        total_bytes,
        elapsed_secs,
        download_speed,
        job_start_time,
        download_start_time,
        end_time,
        time_to_first_byte_ms,
        second_by_second_logs,
    })
}
