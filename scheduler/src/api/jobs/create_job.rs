use axum::{
    debug_handler,
    extract::{Json, State},
};
use axum_extra::extract::WithRejection;
use color_eyre::Result;
use common::api_response::*;
use rand::Rng;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{debug, info};
use url::Url;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    job_repository::{Job, JobDetails, JobStatus},
    state::AppState,
    sub_job_repository::{SubJob, SubJobDetails, SubJobStatus, SubJobType},
};

#[derive(Deserialize, ToSchema, Debug)]
pub struct CreateJobInput {
    #[schema(
        example = "http://yablufc.ddns.net:7878/piece/baga6ea4seaqb4lqf6fzjomlnhn3jahwxg52ewgcbjelzyflqjjuc7by224hbwla"
    )]
    pub url: String,
    #[schema(example = "us_east")]
    pub routing_key: String,
    #[schema(minimum = 1, maximum = 40)]
    pub worker_count: Option<i64>,
    pub entity: Option<String>,
    pub note: Option<String>,
    #[schema(minimum = 10, maximum = 1024)]
    pub size_mb: Option<i64>,
    #[schema(minimum = 100, maximum = 1000)]
    pub log_interval_ms: Option<i64>,
}

#[derive(Serialize, ToSchema)]
pub struct CreateJobResponse {
    #[serde(flatten)]
    pub job: Job,
    pub sub_jobs: Vec<SubJob>,
}

#[derive(Debug)]
struct CreateJobParams {
    pub url: Url,
    pub routing_key: String,
    pub worker_count: i64,
    pub entity: Option<String>,
    pub note: Option<String>,
    pub size_mb: i64,
    pub log_interval_ms: i64,
}

impl TryFrom<CreateJobInput> for CreateJobParams {
    type Error = ApiResponse<()>;

    fn try_from(input: CreateJobInput) -> Result<Self, Self::Error> {
        let url = Url::parse(&input.url).map_err(|_| bad_request("Invalid URL provided"))?;
        if url.scheme() != "http" && url.scheme() != "https" {
            return Err(bad_request("URL scheme must be http or https"));
        }

        if input.routing_key.is_empty() {
            return Err(bad_request("Routing key cannot be empty"));
        }

        Ok(CreateJobParams {
            url,
            routing_key: input.routing_key,
            worker_count: input.worker_count.unwrap_or(10).clamp(1, 40),
            entity: input.entity,
            note: input.note,
            size_mb: input.size_mb.unwrap_or(100).clamp(10, 1024), // Default 100 MB, Possible size 10-1024 MB
            log_interval_ms: input.log_interval_ms.unwrap_or(1000).clamp(100, 1000), // Default 1000 ms, Possible range 100-1000 ms
        })
    }
}

/// Creates a new Job to be processed by the worker
#[utoipa::path(
    post,
    path = "/jobs",
    request_body(content = CreateJobInput),
    description = r#"
**Creates a new Job to be processed by the worker.**

The Job consists of three subjobs:
- **Scaling SubJob**: Facilitates automatic scaling of the workers.
- **Benchmark SubJob 1**: Performs the first part of the benchmark work.
- **Benchmark SubJob 2**: Performs the second part of the benchmark work.

**All subjobs are carried out sequentially.**
    "#,
    responses(
        (status = 200, description = "Job Created", body = CreateJobResponse),
        (status = 400, description = "Bad Request", body = ErrorResponse),
        (status = 500, description = "Internal Server Error", body = ErrorResponse),
    ),
    tags = ["Jobs"],
)]
#[debug_handler]
pub async fn handle_create_job(
    State(state): State<Arc<AppState>>,
    WithRejection(Json(payload), _): WithRejection<
        Json<CreateJobInput>,
        ApiResponse<ErrorResponse>,
    >,
) -> Result<ApiResponse<CreateJobResponse>, ApiResponse<()>> {
    info!("Creating job with payload: {:?}", payload);

    // Validation
    let params: CreateJobParams = payload.try_into()?;
    let target_worker_count = params.worker_count;

    // Create the job
    let (start_range, end_range) =
        get_file_range_for_file(params.url.as_ref(), &params.size_mb).await?;

    let job_id = Uuid::new_v4();

    let job = state
        .repo
        .job
        .create_job(
            job_id,
            params.url.to_string(),
            &params.routing_key,
            JobStatus::Pending,
            JobDetails::new(
                start_range,
                end_range,
                target_worker_count,
                params.entity.clone(),
                params.note.clone(),
                params.log_interval_ms,
                params.size_mb,
            ),
        )
        .await
        .map_err(|_| internal_server_error("Failed to create job"))?;

    debug!("Job created successfully: {:?}", job);

    let scaling_sub_job = state
        .repo
        .sub_job
        .create_sub_job(
            Uuid::new_v4(),
            job.id,
            SubJobStatus::Created,
            SubJobType::Scaling,
            SubJobDetails::topic(job.routing_key.clone()),
        )
        .await
        .map_err(|_| internal_server_error("Failed to create scaling sub job"))?;

    let sub_jobs = vec![
        scaling_sub_job,
        create_sub_job(&state, &job, SubJobDetails::partial(1)).await?,
        create_sub_job(&state, &job, SubJobDetails::partial(80)).await?,
        create_sub_job(&state, &job, SubJobDetails::empty()).await?,
    ];

    debug!(
        "Job with sub jobs created successfully: {}, sub_jobs: {:?}",
        job_id, sub_jobs
    );

    Ok(ok_response(CreateJobResponse { job, sub_jobs }))
}

/// Get a random range of 100MB from the file using HEAD request
async fn get_file_range_for_file(url: &str, size_mb: &i64) -> Result<(i64, i64), ApiResponse<()>> {
    let response = Client::new()
        .head(url)
        .send()
        .await
        .map_err(|e| bad_request(format!("Failed to execute HEAD request {e}")))?;

    debug!("Response: {:?}", response);

    // For some freak reason response.content_length() is returning 0
    let content_length = response
        .headers()
        .get(reqwest::header::CONTENT_LENGTH)
        .ok_or_else(|| bad_request("Content-Length header is missing in the response"))?
        .to_str()
        .map_err(|e| bad_request(format!("Failed to parse Content-Length header: {e}")))?
        .parse::<i64>()
        .map_err(|e| bad_request(format!("Failed to parse Content-Length header: {e}")))?;

    debug!("Content-Length: {:?}", content_length);

    let size = size_mb * 1024 * 1024;

    if content_length < size {
        return Err(bad_request(format!("File size is less than {size_mb} MB")));
    }

    let mut rng = rand::thread_rng();
    let start_range = rng.gen_range(0..content_length - size);
    let end_range = start_range + size;

    debug!("Selected range: {} - {}", start_range, end_range);

    Ok((start_range, end_range))
}

async fn create_sub_job(
    state: &Arc<AppState>,
    job: &Job,
    details: SubJobDetails,
) -> Result<SubJob, ApiResponse<()>> {
    let sub_job = state
        .repo
        .sub_job
        .create_sub_job(
            Uuid::new_v4(),
            job.id,
            SubJobStatus::Created,
            SubJobType::CombinedDHP,
            details,
        )
        .await
        .map_err(|_| internal_server_error("Failed to create sub job"))?;

    debug!("Sub job created successfully: {:?}", sub_job);

    Ok(sub_job)
}
