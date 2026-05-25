// scheduler/tests/integration/geolocation.rs

use crate::common::*;
use chrono::Utc;
use rabbitmq::{get_publisher_config, ConnectionManager, Publisher, PublisherType, WorkerStatus};
use scheduler::{
    background::process_pending_sub_jobs,
    repository::Repositories,
    service_repository::ProviderType,
    service_scaler::{ServiceScaler, ServiceScalerRegistry},
};
use sqlx::Row;
use std::{collections::HashMap, sync::Arc};
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
    let job = sqlx::query(r#"SELECT job_type::text as job_type FROM jobs WHERE id = $1"#)
        .bind(job_id)
        .fetch_one(ctx.pool())
        .await
        .unwrap();

    assert_eq!(
        job.get::<Option<String>, _>("job_type"),
        Some("geolocation".to_string())
    );

    // Verify sub-jobs: should be 3 scaling + 3 benchmarks = 6 total
    let sub_jobs = sqlx::query(
        r#"SELECT type::text as sub_type, details->>'topic' as topic
           FROM sub_jobs WHERE job_id = $1"#,
    )
    .bind(job_id)
    .fetch_all(ctx.pool())
    .await
    .unwrap();

    assert_eq!(
        sub_jobs.len(),
        6,
        "Should have 6 sub-jobs (3 scaling + 3 benchmarks)"
    );

    // Count by type (enum values are PascalCase in database)
    let scaling_count = sub_jobs
        .iter()
        .filter(|s| s.get::<Option<String>, _>("sub_type") == Some("Scaling".to_string()))
        .count();
    let benchmark_count = sub_jobs
        .iter()
        .filter(|s| s.get::<Option<String>, _>("sub_type") == Some("CombinedDHP".to_string()))
        .count();

    assert_eq!(scaling_count, 3, "Should have 3 scaling sub-jobs");
    assert_eq!(benchmark_count, 3, "Should have 3 benchmark sub-jobs");

    // Verify benchmark topics match locations
    let benchmark_topics: Vec<String> = sub_jobs
        .iter()
        .filter(|s| s.get::<Option<String>, _>("sub_type") == Some("CombinedDHP".to_string()))
        .filter_map(|s| s.get::<Option<String>, _>("topic"))
        .collect();

    assert!(benchmark_topics.contains(&"europe".to_string()));
    assert!(benchmark_topics.contains(&"usa".to_string()));
    assert!(benchmark_topics.contains(&"asia".to_string()));

    // Verify scaling topics match locations
    let scaling_topics: Vec<String> = sub_jobs
        .iter()
        .filter(|s| s.get::<Option<String>, _>("sub_type") == Some("Scaling".to_string()))
        .filter_map(|s| s.get::<Option<String>, _>("topic"))
        .collect();

    assert!(scaling_topics.contains(&"europe".to_string()));
    assert!(scaling_topics.contains(&"usa".to_string()));
    assert!(scaling_topics.contains(&"asia".to_string()));
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
    sqlx::query("UPDATE services SET location = NULL")
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
async fn test_geolocation_job_with_bad_service_entries_creates_failed_job() {
    let ctx = TestContext::new().await;

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
        "worker_eu_es",
        "spain",
        "docker_local",
        vec!["all", "europe", "spain"],
    )
    .await;

    ctx.file_server
        .setup_file("/testfile", 100 * 1024 * 1024, None)
        .await;

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
    assert_eq!(body["status"], serde_json::json!("failed"));

    let job = sqlx::query(
        r#"
        SELECT status::text as status
        FROM jobs
        WHERE id = $1
        "#,
    )
    .bind(job_id)
    .fetch_one(ctx.pool())
    .await
    .unwrap();

    assert_eq!(
        job.get::<Option<String>, _>("status"),
        Some("Failed".to_string())
    );

    let sub_jobs = sqlx::query(
        r#"
        SELECT status::text as status, details
        FROM sub_jobs
        WHERE job_id = $1
        "#,
    )
    .bind(job_id)
    .fetch_all(ctx.pool())
    .await
    .unwrap();

    assert_eq!(sub_jobs.len(), 4);

    for sub_job in sub_jobs {
        assert_eq!(
            sub_job.get::<Option<String>, _>("status"),
            Some("Failed".to_string())
        );
        let details = sub_job.get::<serde_json::Value, _>("details");
        let error_msg = details
            .get("error")
            .and_then(|value| value.as_str())
            .unwrap_or("");
        assert!(
            error_msg.contains("Invalid geolocation service configuration")
                || error_msg.contains("Location 'europe' topic is also attached"),
            "expected invalid service configuration error, got: {}",
            error_msg
        );
    }

    let response = ctx.app.get(&format!("/jobs/{job_id}")).await;
    response.assert_status_ok();

    let body: serde_json::Value = response.json();
    assert!(
        body["completion"]["failed"].as_i64().unwrap_or_default() >= 1,
        "expected failed geolocation locations, got: {}",
        body
    );
}

#[tokio::test]
async fn test_geolocation_scaling_failure_fails_matching_benchmark_and_parent_job() {
    let ctx = TestContext::new().await;

    seed_service_with_topics(
        ctx.pool(),
        "worker_eu",
        "europe",
        "docker_local",
        vec!["all", "europe"],
    )
    .await;

    ctx.file_server
        .setup_file("/testfile", 100 * 1024 * 1024, None)
        .await;

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

    ctx.mock_scaler.fail_next_scale_up().await;

    let repo = Arc::new(Repositories::new(ctx.pool().clone()));
    let mut scalers: HashMap<ProviderType, Arc<dyn ServiceScaler>> = HashMap::new();
    scalers.insert(ProviderType::DockerLocal, ctx.mock_scaler.clone());
    let service_scaler_registry = Arc::new(ServiceScalerRegistry::new_with_scalers(scalers));
    let job_queue = Arc::new(Publisher::new(
        get_publisher_config(PublisherType::JobPublisher),
        Arc::new(ConnectionManager::new()),
    ));

    process_pending_sub_jobs(&repo, &job_queue, &service_scaler_registry)
        .await
        .unwrap();

    let sub_jobs = sqlx::query(
        r#"
        SELECT type::text as sub_type, status::text as status, details
        FROM sub_jobs
        WHERE job_id = $1
        ORDER BY type::text
        "#,
    )
    .bind(job_id)
    .fetch_all(ctx.pool())
    .await
    .unwrap();

    let scaling = sub_jobs
        .iter()
        .find(|sub_job| sub_job.get::<Option<String>, _>("sub_type") == Some("Scaling".to_string()))
        .expect("scaling sub-job should exist");
    assert_eq!(
        scaling.get::<Option<String>, _>("status"),
        Some("Failed".to_string())
    );

    let benchmark = sub_jobs
        .iter()
        .find(|sub_job| {
            sub_job.get::<Option<String>, _>("sub_type") == Some("CombinedDHP".to_string())
        })
        .expect("benchmark sub-job should exist");
    assert_eq!(
        benchmark.get::<Option<String>, _>("status"),
        Some("Failed".to_string())
    );

    let benchmark_details = benchmark.get::<serde_json::Value, _>("details");
    let benchmark_error = benchmark_details
        .get("error")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    assert!(
        benchmark_error.contains("Scaling failed")
            || benchmark_error.contains("mock scale up failed"),
        "expected benchmark error to mention scaling failure, got: {}",
        benchmark_error
    );

    let job = sqlx::query(
        r#"
        SELECT status::text as status
        FROM jobs
        WHERE id = $1
        "#,
    )
    .bind(job_id)
    .fetch_one(ctx.pool())
    .await
    .unwrap();

    assert_eq!(
        job.get::<Option<String>, _>("status"),
        Some("Failed".to_string())
    );
}

#[tokio::test]
async fn test_geolocation_failed_worker_result_fails_benchmark_and_parent_job() {
    let ctx = TestContext::new().await;

    seed_service_with_topics(
        ctx.pool(),
        "worker_eu",
        "europe",
        "docker_local",
        vec!["all", "europe"],
    )
    .await;

    ctx.file_server
        .setup_file("/testfile", 100 * 1024 * 1024, None)
        .await;

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

    sqlx::query(
        r#"
        UPDATE sub_jobs
        SET status = 'Completed'
        WHERE job_id = $1
          AND type = 'Scaling'
        "#,
    )
    .bind(job_id)
    .execute(ctx.pool())
    .await
    .unwrap();

    let benchmark = sqlx::query(
        r#"
        UPDATE sub_jobs
        SET
            status = 'Processing',
            details = details || jsonb_build_object('workers_count', 1),
            deadline_at = NOW() + INTERVAL '1 hour'
        WHERE job_id = $1
          AND type = 'CombinedDHP'
        RETURNING id
        "#,
    )
    .bind(job_id)
    .fetch_one(ctx.pool())
    .await
    .unwrap();
    let benchmark_id = benchmark.get::<Uuid, _>("id");

    let repo = Arc::new(Repositories::new(ctx.pool().clone()));
    let worker_name = "worker_eu".to_string();
    repo.worker
        .update_worker_status(&worker_name, &WorkerStatus::Online, Utc::now(), None)
        .await
        .unwrap();

    repo.data
        .save_data(create_error_result(
            Uuid::new_v4(),
            job_id,
            benchmark_id,
            &worker_name,
            "download failed",
        ))
        .await
        .unwrap();

    let mut scalers: HashMap<ProviderType, Arc<dyn ServiceScaler>> = HashMap::new();
    scalers.insert(ProviderType::DockerLocal, ctx.mock_scaler.clone());
    let service_scaler_registry = Arc::new(ServiceScalerRegistry::new_with_scalers(scalers));
    let job_queue = Arc::new(Publisher::new(
        get_publisher_config(PublisherType::JobPublisher),
        Arc::new(ConnectionManager::new()),
    ));

    process_pending_sub_jobs(&repo, &job_queue, &service_scaler_registry)
        .await
        .unwrap();

    let sub_job = sqlx::query(
        r#"
        SELECT status::text as status, details
        FROM sub_jobs
        WHERE id = $1
        "#,
    )
    .bind(benchmark_id)
    .fetch_one(ctx.pool())
    .await
    .unwrap();

    assert_eq!(
        sub_job.get::<Option<String>, _>("status"),
        Some("Failed".to_string())
    );

    let sub_job_details = sub_job.get::<serde_json::Value, _>("details");
    let error = sub_job_details
        .get("error")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    assert!(
        error.contains("All geolocation worker downloads failed"),
        "expected geolocation worker failure error, got: {}",
        error
    );

    let job = sqlx::query(
        r#"
        SELECT status::text as status
        FROM jobs
        WHERE id = $1
        "#,
    )
    .bind(job_id)
    .fetch_one(ctx.pool())
    .await
    .unwrap();

    assert_eq!(
        job.get::<Option<String>, _>("status"),
        Some("Failed".to_string())
    );

    let response = ctx.app.get(&format!("/jobs/{job_id}")).await;
    response.assert_status_ok();

    let body: serde_json::Value = response.json();
    assert_eq!(body["completion"]["failed"], serde_json::json!(1));
    assert_eq!(body["completion"]["is_partial"], serde_json::json!(true));
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
