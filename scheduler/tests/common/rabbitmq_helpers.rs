// scheduler/tests/common/rabbitmq_helpers.rs

use chrono::Utc;
use rabbitmq::{
    AccumulatingBytes, DownloadResult, HeadResult, IntervalBytes, PingResult, ResultMessage,
};
use uuid::Uuid;

/// Creates a successful ResultMessage with download, ping, and head results
pub fn create_success_result(
    run_id: Uuid,
    job_id: Uuid,
    sub_job_id: Uuid,
    worker_name: &str,
) -> ResultMessage {
    let now = Utc::now();
    let start_time = now - chrono::Duration::seconds(10);

    ResultMessage {
        run_id,
        job_id,
        sub_job_id,
        worker_name: worker_name.to_string(),
        is_success: true,
        download_result: Ok(DownloadResult {
            total_bytes: 104_857_600, // 100 MB
            elapsed_secs: 10.0,
            download_speed: 83.886, // ~80 Mbps
            job_start_time: start_time,
            download_start_time: start_time + chrono::Duration::milliseconds(50),
            end_time: now,
            time_to_first_byte_ms: 50.0,
            second_by_second_logs: vec![
                (
                    start_time + chrono::Duration::seconds(1),
                    IntervalBytes(10_485_760),
                    AccumulatingBytes(10_485_760),
                ),
                (
                    start_time + chrono::Duration::seconds(2),
                    IntervalBytes(10_485_760),
                    AccumulatingBytes(20_971_520),
                ),
            ],
        }),
        ping_result: Ok(PingResult {
            min: 10.0,
            max: 15.0,
            avg: 12.5,
            ip_address: "192.168.1.1".to_string(),
        }),
        head_result: Ok(HeadResult {
            min: 5.0,
            max: 8.0,
            avg: 6.5,
        }),
    }
}

/// Creates a failed ResultMessage using ResultMessage::aborted()
pub fn create_error_result(
    run_id: Uuid,
    job_id: Uuid,
    sub_job_id: Uuid,
    worker_name: &str,
    error: &str,
) -> ResultMessage {
    ResultMessage::aborted(
        run_id,
        job_id,
        sub_job_id,
        worker_name.to_string(),
        error.to_string(),
    )
}

/// Serializes a ResultMessage to JSON
pub fn serialize_result_message(result: &ResultMessage) -> Result<String, serde_json::Error> {
    serde_json::to_string(result)
}
