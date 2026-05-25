use bytes::Bytes;
use chrono::{DateTime, Duration, Utc};
use color_eyre::{eyre::bail, Result};
use rabbitmq::{AccumulatingBytes, DownloadError, DownloadResult, IntervalBytes, JobMessage};
use reqwest::{
    header::{ACCEPT, RANGE, USER_AGENT},
    redirect::Policy,
    Client, Response,
};
use tokio::time::{sleep, timeout};
use tracing::{debug, error, info};
use uuid::Uuid;

// Download deadline, job will succeed but won't work/download more than this duration
const MAX_DOWNLOAD_DURATION: Duration = Duration::seconds(60);

/// Prepare the HTTP request
fn prepare_request(
    url: &str,
    range_start: i64,
    range_end: i64,
) -> Result<reqwest::RequestBuilder, reqwest::Error> {
    const USER_AGENT_STR: &str = "curl/7.68.0";
    const ACCEPT_TYPE: &str = "*/*";

    let client = Client::builder().redirect(Policy::none()).build()?;

    Ok(client
        .get(url)
        .header(RANGE, format!("bytes={range_start}-{range_end}"))
        .header(USER_AGENT, USER_AGENT_STR)
        .header(ACCEPT, ACCEPT_TYPE))
}

/// Calculates the next interval based on the current time and the specified interval in milliseconds.
fn calculate_next_interval(current: DateTime<Utc>, interval_milis: i64) -> DateTime<Utc> {
    let millis = current.timestamp_millis() % interval_milis;
    let remaining_millis = interval_milis - millis;

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
            error: format!("ChunkError: {e}"),
        }),
        _ => Ok(None),
    }
}

/// Benchmark the download speed of the given URL
#[tracing::instrument(skip(payload))]
pub async fn process(job_id: Uuid, payload: JobMessage) -> Result<DownloadResult, DownloadError> {
    info!("Processing Download job");

    crate::url_security::assert_public_http_url(&payload.url)
        .await
        .map_err(|e| DownloadError {
            error: format!("UrlSecurityError: {e}"),
        })?;

    let request =
        prepare_request(&payload.url, payload.start_range, payload.end_range).map_err(|e| {
            DownloadError {
                error: format!("ClientBuildError: {e}"),
            }
        })?;

    let job_start_time = Utc::now();
    let mut bytes: usize = 0;
    let mut total_bytes: usize = 0;
    let mut second_by_second_logs: Vec<(DateTime<Utc>, IntervalBytes, AccumulatingBytes)> =
        Vec::new();

    // Delay the download execution to sync the time on every worker
    wait_for_start_time(&payload)
        .await
        .map_err(|e| DownloadError {
            error: format!("TimeSyncError: {e}"),
        })?;

    let mut response = request.send().await.map_err(|e| DownloadError {
        error: format!("RequestError: {e}"),
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
    let mut next_log_time = calculate_next_interval(download_start_time, payload.log_interval_ms);

    debug!(
        "job_start_time: {}, download_start_time: {}, next_log_time: {}, log_interval_ms: {}",
        job_start_time, download_start_time, next_log_time, payload.log_interval_ms
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
            next_log_time = calculate_next_interval(current_time, payload.log_interval_ms);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_next_interval_already_aligned() {
        // Time already at an interval boundary (Jan 1, 2023 00:00:00 UTC)
        let current = DateTime::<Utc>::from_timestamp_millis(1672531200000).unwrap();
        let interval = 1000; // 1 second
        let result = calculate_next_interval(current, interval);

        // Should go to the next interval boundary (1 second later)
        let expected = current + Duration::milliseconds(1000);
        assert_eq!(result, expected);
    }

    #[test]
    fn test_calculate_next_interval_not_aligned() {
        // Time not at an interval boundary (Jan 1, 2023 00:00:00.5 UTC)
        let current = DateTime::<Utc>::from_timestamp_millis(1672531200500).unwrap();
        let interval = 1000; // 1 second
        let result = calculate_next_interval(current, interval);

        // Should be at the next interval boundary (500ms later)
        let expected = current + Duration::milliseconds(500);
        assert_eq!(result, expected);
    }

    #[test]
    fn test_calculate_next_interval_custom_interval() {
        // Test with a custom interval (Jan 1, 2023 00:00:00.1 UTC)
        let current = DateTime::<Utc>::from_timestamp_millis(1672531200100).unwrap();
        let interval = 250; // 250ms
        let result = calculate_next_interval(current, interval);

        // Should be at the next interval boundary (150ms later)
        let expected = current + Duration::milliseconds(150);
        assert_eq!(result, expected);
    }

    #[test]
    fn test_calculate_next_interval_large_interval() {
        // Test with a large interval (Jan 1, 2023 00:30:00 UTC)
        let current = DateTime::<Utc>::from_timestamp_millis(1672533000000).unwrap();
        let interval = 3_600_000; // 1 hour in milliseconds
        let result = calculate_next_interval(current, interval);

        // Should be at the next hour (Jan 1, 2023 01:00:00 UTC)
        let expected = DateTime::<Utc>::from_timestamp_millis(1672534800000).unwrap();
        assert_eq!(result, expected);
    }

    #[test]
    fn test_calculate_next_interval_100ms_granularity() {
        // Base time: Jan 1, 2023 00:00:00.000 UTC
        let base_time = DateTime::<Utc>::from_timestamp_millis(1672531200000).unwrap();
        let interval = 100; // 100ms interval

        // Test with different offsets within the 100ms interval

        // Case 1: At 0ms offset (aligned)
        let current = base_time;
        let result = calculate_next_interval(current, interval);
        let expected = current + Duration::milliseconds(100);
        assert_eq!(result, expected, "Failed with 0ms offset");

        // Case 2: At 10ms offset
        let current = base_time + Duration::milliseconds(10);
        let result = calculate_next_interval(current, interval);
        let expected = current + Duration::milliseconds(90);
        assert_eq!(result, expected, "Failed with 10ms offset");

        // Case 3: At 50ms offset (middle of interval)
        let current = base_time + Duration::milliseconds(50);
        let result = calculate_next_interval(current, interval);
        let expected = current + Duration::milliseconds(50);
        assert_eq!(result, expected, "Failed with 50ms offset");

        // Case 4: At 99ms offset (just before next interval)
        let current = base_time + Duration::milliseconds(99);
        let result = calculate_next_interval(current, interval);
        let expected = current + Duration::milliseconds(1);
        assert_eq!(result, expected, "Failed with 99ms offset");
    }
}
