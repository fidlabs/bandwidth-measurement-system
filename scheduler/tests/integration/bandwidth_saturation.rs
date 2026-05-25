// scheduler/tests/integration/bandwidth_saturation.rs

use crate::common::*;
use sqlx::Row;
use uuid::Uuid;

#[tokio::test]
async fn test_create_bandwidth_job_returns_job_id() {
    let ctx = TestContext::new().await;

    // Seed required data
    seed_service_with_topics(
        ctx.pool(),
        "worker_eu_pl",
        "europe",
        "docker_local",
        vec!["all", "europe"],
    )
    .await;

    // Setup mock file server
    ctx.file_server
        .setup_file("/testfile", 100 * 1024 * 1024, None)
        .await;

    // Create job
    let response = ctx
        .app
        .post("/jobs")
        .json(&serde_json::json!({
            "url": ctx.file_server.url("/testfile"),
            "routing_key": "europe",
            "worker_count": 1,
            "size_mb": 10
        }))
        .await;

    response.assert_status_ok();

    let body: serde_json::Value = response.json();
    // Response contains full job object with "id" field
    assert!(
        body.get("id").is_some(),
        "Response should contain id: {:?}",
        body
    );
}

#[tokio::test]
async fn test_bandwidth_job_creates_three_sub_jobs() {
    let ctx = TestContext::new().await;

    // Seed required data
    seed_service_with_topics(
        ctx.pool(),
        "worker_test",
        "europe",
        "docker_local",
        vec!["all", "europe"],
    )
    .await;

    ctx.file_server
        .setup_file("/testfile", 100 * 1024 * 1024, None)
        .await;

    // Create bandwidth job
    let response = ctx
        .app
        .post("/jobs")
        .json(&serde_json::json!({
            "url": ctx.file_server.url("/testfile"),
            "routing_key": "europe",
            "worker_count": 3,
            "size_mb": 10
        }))
        .await;

    response.assert_status_ok();
    let body: serde_json::Value = response.json();
    let job_id: Uuid = body["id"].as_str().unwrap().parse().unwrap();

    // Verify job type
    let job = sqlx::query(r#"SELECT job_type::text as job_type FROM jobs WHERE id = $1"#)
        .bind(job_id)
        .fetch_one(ctx.pool())
        .await
        .unwrap();

    assert_eq!(
        job.get::<Option<String>, _>("job_type"),
        Some("bandwidth_saturation".to_string())
    );

    // Verify sub-jobs: 1 Scaling + 3 CombinedDHP (1%, 80%, 100%) = 4 total
    let sub_jobs = sqlx::query(
        r#"SELECT type::text as sub_type, details->>'worker_percent' as worker_percent
           FROM sub_jobs WHERE job_id = $1 ORDER BY created_at"#,
    )
    .bind(job_id)
    .fetch_all(ctx.pool())
    .await
    .unwrap();

    assert_eq!(
        sub_jobs.len(),
        4,
        "Should have 4 sub-jobs (1 Scaling + 3 CombinedDHP)"
    );

    // Count by type
    let scaling_count = sub_jobs
        .iter()
        .filter(|s| s.get::<Option<String>, _>("sub_type") == Some("Scaling".to_string()))
        .count();
    let benchmark_count = sub_jobs
        .iter()
        .filter(|s| s.get::<Option<String>, _>("sub_type") == Some("CombinedDHP".to_string()))
        .count();

    assert_eq!(scaling_count, 1, "Should have 1 Scaling sub-job");
    assert_eq!(benchmark_count, 3, "Should have 3 CombinedDHP sub-jobs");
}
