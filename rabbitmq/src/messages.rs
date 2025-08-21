use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// Messages that can be sent or received
#[derive(Serialize, Deserialize, Debug)]
pub enum Message {
    WorkerJob { job_id: Uuid, payload: JobMessage },
    WorkerResult { job_id: Uuid, result: ResultMessage },
    WorkerStatus { status: StatusMessage },
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct JobMessage {
    pub job_id: Uuid,
    pub sub_job_id: Uuid,
    pub url: String,
    pub start_time: DateTime<Utc>,
    pub download_start_time: DateTime<Utc>,
    pub start_range: i64,
    pub end_range: i64,
    pub excluded_workers: Vec<String>,
    pub log_interval_ms: i64,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ResultMessage {
    pub run_id: Uuid,
    pub job_id: Uuid,
    pub sub_job_id: Uuid,
    pub worker_name: String,
    pub is_success: bool,
    pub download_result: Result<DownloadResult, DownloadError>,
    pub ping_result: Result<PingResult, PingError>,
    pub head_result: Result<HeadResult, HeadError>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DownloadResult {
    pub total_bytes: usize,
    pub elapsed_secs: f64,
    pub download_speed: f64,
    pub job_start_time: DateTime<Utc>,
    pub download_start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
    pub time_to_first_byte_ms: f64,
    pub second_by_second_logs: Vec<(DateTime<Utc>, IntervalBytes, AccumulatingBytes)>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct IntervalBytes(pub usize);
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AccumulatingBytes(pub usize);

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DownloadError {
    pub error: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PingResult {
    pub min: f64,
    pub max: f64,
    pub avg: f64,
    pub ip_address: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PingError {
    pub error: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct HeadResult {
    pub min: f64,
    pub max: f64,
    pub avg: f64,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct HeadError {
    pub error: String,
}

impl ResultMessage {
    pub fn aborted(
        run_id: Uuid,
        job_id: Uuid,
        sub_job_id: Uuid,
        worker_name: String,
        error: String,
    ) -> Self {
        Self {
            run_id,
            job_id,
            sub_job_id,
            worker_name,
            is_success: false,
            download_result: Err(DownloadError {
                error: error.clone(),
            }),
            ping_result: Err(PingError {
                error: error.clone(),
            }),
            head_result: Err(HeadError { error }),
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct WorkerStatusJobDetails {
    pub run_id: Uuid,
    pub job_id: Uuid,
    pub sub_job_id: Uuid,
    pub worker_name: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum WorkerStatusDetails {
    Lifecycle(WorkerDetails),
    Job(Option<WorkerStatusJobDetails>),
    Heartbeat,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct WorkerDetails {
    pub worker_topics: Vec<String>,
    pub worker_status: WorkerStatus,
}
#[derive(Serialize, Deserialize, Debug)]
pub enum WorkerStatus {
    Online,
    Offline,
}
impl WorkerStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            WorkerStatus::Online => "online",
            WorkerStatus::Offline => "offline",
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct StatusMessage {
    pub worker_name: String,
    pub status: WorkerStatusDetails,
    pub timestamp: DateTime<Utc>,
}
