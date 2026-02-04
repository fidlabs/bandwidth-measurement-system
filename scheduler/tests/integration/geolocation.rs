// scheduler/tests/integration/geolocation.rs

use crate::common::*;
use uuid::Uuid;

#[tokio::test]
async fn test_create_geolocation_job_returns_job_id() {
    let ctx = TestContext::new().await;

    // Seed services with different locations
    seed_service_with_topics(
        ctx.pool(),
        "worker_eu_pl",
        "europe",
        "docker_local",
        vec!["all", "europe"],
    )
    .await;

    seed_service_with_topics(
        ctx.pool(),
        "worker_usa_la",
        "usa",
        "docker_local",
        vec!["all", "usa"],
    )
    .await;

    // Setup mock file server with a file larger than the job's size_mb.
    // Workers use byte-range requests to download only the requested portion,
    // so the file must be at least as large as size_mb (10 MB in this case).
    ctx.file_server
        .setup_file("/testfile", 100 * 1024 * 1024, None)
        .await;

    // Create geolocation job
    let response = ctx
        .app
        .post("/jobs/geolocation")
        .json(&serde_json::json!({
            "url": ctx.file_server.url("/testfile"),
            "size_mb": 10
        }))
        .await;

    response.assert_status_ok();

    let body: serde_json::Value = response.json();
    // Response contains job_id (different from bandwidth_saturation endpoint which returns full job)
    assert!(
        body.get("job_id").is_some(),
        "Response should contain job_id: {:?}",
        body
    );
}

#[tokio::test]
async fn test_geolocation_job_creates_correct_sub_jobs() {
    let ctx = TestContext::new().await;

    // Seed 3 services with different locations
    seed_service_with_topics(
        ctx.pool(),
        "worker_eu",
        "europe",
        "docker_local",
        vec!["all", "europe"],
    )
    .await;

    seed_service_with_topics(
        ctx.pool(),
        "worker_us",
        "usa",
        "docker_local",
        vec!["all", "usa"],
    )
    .await;

    seed_service_with_topics(
        ctx.pool(),
        "worker_asia",
        "asia",
        "docker_local",
        vec!["all", "asia"],
    )
    .await;

    ctx.file_server
        .setup_file("/testfile", 100 * 1024 * 1024, None)
        .await;

    // Create geolocation job
    let response = ctx
        .app
        .post("/jobs/geolocation")
        .json(&serde_json::json!({
            "url": ctx.file_server.url("/testfile"),
            "size_mb": 10
        }))
        .await;

    response.assert_status_ok();
    let body: serde_json::Value = response.json();
    let job_id: Uuid = body["job_id"].as_str().unwrap().parse().unwrap();

    // Verify job type
    let job = sqlx::query!(
        r#"SELECT job_type::text as job_type FROM jobs WHERE id = $1"#,
        job_id
    )
    .fetch_one(ctx.pool())
    .await
    .unwrap();

    assert_eq!(job.job_type, Some("geolocation".to_string()));

    // Verify sub-jobs: should be 1 scaling + 3 benchmarks = 4 total
    let sub_jobs = sqlx::query!(
        r#"SELECT type::text as sub_type, details->>'topic' as topic
           FROM sub_jobs WHERE job_id = $1"#,
        job_id
    )
    .fetch_all(ctx.pool())
    .await
    .unwrap();

    assert_eq!(
        sub_jobs.len(),
        4,
        "Should have 4 sub-jobs (1 scaling + 3 benchmarks)"
    );

    // Count by type (enum values are PascalCase in database)
    let scaling_count = sub_jobs
        .iter()
        .filter(|s| s.sub_type == Some("Scaling".to_string()))
        .count();
    let benchmark_count = sub_jobs
        .iter()
        .filter(|s| s.sub_type == Some("CombinedDHP".to_string()))
        .count();

    assert_eq!(scaling_count, 1, "Should have 1 scaling sub-job");
    assert_eq!(benchmark_count, 3, "Should have 3 benchmark sub-jobs");

    // Verify benchmark topics match locations
    let benchmark_topics: Vec<String> = sub_jobs
        .iter()
        .filter(|s| s.sub_type == Some("CombinedDHP".to_string()))
        .filter_map(|s| s.topic.clone())
        .collect();

    assert!(benchmark_topics.contains(&"europe".to_string()));
    assert!(benchmark_topics.contains(&"usa".to_string()));
    assert!(benchmark_topics.contains(&"asia".to_string()));
}

#[tokio::test]
async fn test_geolocation_job_fails_without_locations() {
    let ctx = TestContext::new().await;

    // Seed service WITHOUT location
    seed_service_with_topics(
        ctx.pool(),
        "worker_no_location",
        "", // empty location - will be treated as no location
        "docker_local",
        vec!["all"],
    )
    .await;

    // Clear the location to NULL
    sqlx::query!("UPDATE services SET location = NULL")
        .execute(ctx.pool())
        .await
        .unwrap();

    ctx.file_server
        .setup_file("/testfile", 100 * 1024 * 1024, None)
        .await;

    // Create geolocation job - should fail
    let response = ctx
        .app
        .post("/jobs/geolocation")
        .json(&serde_json::json!({
            "url": ctx.file_server.url("/testfile"),
            "size_mb": 10
        }))
        .await;

    response.assert_status_bad_request();

    let body: serde_json::Value = response.json();
    let error_msg = body["error"].as_str().unwrap_or("");
    assert!(
        error_msg.contains("No locations configured"),
        "Should return 'No locations configured' error, got: {}",
        error_msg
    );
}

#[tokio::test]
async fn test_geolocation_job_fails_with_invalid_url() {
    let ctx = TestContext::new().await;

    seed_service_with_topics(
        ctx.pool(),
        "worker_eu",
        "europe",
        "docker_local",
        vec!["all", "europe"],
    )
    .await;

    // Invalid URL format
    let response = ctx
        .app
        .post("/jobs/geolocation")
        .json(&serde_json::json!({
            "url": "not-a-valid-url",
            "size_mb": 10
        }))
        .await;

    response.assert_status_bad_request();

    let body: serde_json::Value = response.json();
    let error_msg = body["error"].as_str().unwrap_or("");
    assert!(
        error_msg.contains("Invalid URL"),
        "Should return 'Invalid URL' error, got: {}",
        error_msg
    );
}

#[tokio::test]
async fn test_geolocation_job_fails_with_small_file() {
    let ctx = TestContext::new().await;

    seed_service_with_topics(
        ctx.pool(),
        "worker_eu",
        "europe",
        "docker_local",
        vec!["all", "europe"],
    )
    .await;

    // File is 5 MB but we request 10 MB
    ctx.file_server
        .setup_file("/smallfile", 5 * 1024 * 1024, None)
        .await;

    let response = ctx
        .app
        .post("/jobs/geolocation")
        .json(&serde_json::json!({
            "url": ctx.file_server.url("/smallfile"),
            "size_mb": 10
        }))
        .await;

    response.assert_status_bad_request();

    let body: serde_json::Value = response.json();
    let error_msg = body["error"].as_str().unwrap_or("");
    assert!(
        error_msg.contains("less than"),
        "Should return file size error, got: {}",
        error_msg
    );
}

#[tokio::test]
async fn test_geolocation_job_fails_with_nonexistent_file() {
    let ctx = TestContext::new().await;

    seed_service_with_topics(
        ctx.pool(),
        "worker_eu",
        "europe",
        "docker_local",
        vec!["all", "europe"],
    )
    .await;

    // Don't setup file - URL will 404
    let response = ctx
        .app
        .post("/jobs/geolocation")
        .json(&serde_json::json!({
            "url": ctx.file_server.url("/nonexistent"),
            "size_mb": 10
        }))
        .await;

    response.assert_status_bad_request();

    let body: serde_json::Value = response.json();
    let error_msg = body["error"].as_str().unwrap_or("");
    assert!(
        error_msg.contains("status 404") || error_msg.contains("Not Found"),
        "Should return 404 error, got: {}",
        error_msg
    );
}
