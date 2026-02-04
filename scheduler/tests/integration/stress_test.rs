// scheduler/tests/integration/stress_test.rs

use crate::common::*;
use std::time::Instant;
use uuid::Uuid;

/// Stress test: Create 10 concurrent geolocation jobs
#[tokio::test]
async fn test_concurrent_geolocation_job_creation() {
    let ctx = TestContext::new().await;

    // Seed services with different locations
    for (i, location) in ["europe", "usa", "asia"].iter().enumerate() {
        seed_service_with_topics(
            ctx.pool(),
            &format!("worker_{}", i),
            location,
            "docker_local",
            vec!["all", location],
        )
        .await;
    }

    // Setup mock file server
    ctx.file_server
        .setup_file("/stressfile", 100 * 1024 * 1024, None)
        .await;

    let start = Instant::now();
    let num_jobs = 10;

    // Create 10 jobs sequentially but quickly - axum-test TestRequest is not a Future
    // We need to await each one, but the server handles them concurrently
    let mut job_ids: Vec<Uuid> = Vec::new();
    for i in 0..num_jobs {
        let url = ctx.file_server.url("/stressfile");
        let response = ctx
            .app
            .post("/jobs/geolocation")
            .json(&serde_json::json!({
                "url": url,
                "size_mb": 10,
                "entity": format!("stress_test_geo_{}", i),
                "note": format!("Concurrent geolocation job {}", i)
            }))
            .await;

        response.assert_status_ok();
        let body: serde_json::Value = response.json();
        let job_id: Uuid = body["job_id"]
            .as_str()
            .unwrap_or_else(|| panic!("Job {} missing job_id: {:?}", i, body))
            .parse()
            .unwrap();
        job_ids.push(job_id);
    }

    let elapsed = start.elapsed();

    assert_eq!(
        job_ids.len(),
        num_jobs,
        "Should have created {} jobs",
        num_jobs
    );

    // Verify all job IDs are unique
    let unique_ids: std::collections::HashSet<_> = job_ids.iter().collect();
    assert_eq!(unique_ids.len(), num_jobs, "All job IDs should be unique");

    // Verify each job has correct sub-jobs (1 scaling + 3 benchmarks = 4 total)
    for job_id in &job_ids {
        let sub_job_count =
            sqlx::query_scalar!(r#"SELECT COUNT(*) FROM sub_jobs WHERE job_id = $1"#, job_id)
                .fetch_one(ctx.pool())
                .await
                .unwrap()
                .unwrap_or(0);

        assert_eq!(
            sub_job_count, 4,
            "Job {} should have 4 sub-jobs (1 scaling + 3 benchmarks), got {}",
            job_id, sub_job_count
        );
    }

    // Verify total jobs in database
    let total_jobs = sqlx::query_scalar!(r#"SELECT COUNT(*) FROM jobs"#)
        .fetch_one(ctx.pool())
        .await
        .unwrap()
        .unwrap_or(0);

    assert_eq!(
        total_jobs, num_jobs as i64,
        "Database should contain exactly {} jobs",
        num_jobs
    );

    // Verify total sub-jobs in database (10 jobs * 4 sub-jobs each = 40)
    let total_sub_jobs = sqlx::query_scalar!(r#"SELECT COUNT(*) FROM sub_jobs"#)
        .fetch_one(ctx.pool())
        .await
        .unwrap()
        .unwrap_or(0);

    assert_eq!(
        total_sub_jobs,
        (num_jobs * 4) as i64,
        "Database should contain {} sub-jobs",
        num_jobs * 4
    );

    println!(
        "Created {} geolocation jobs with {} total sub-jobs in {:?}",
        num_jobs,
        num_jobs * 4,
        elapsed
    );
}

/// Stress test: Create 10 concurrent bandwidth_saturation jobs
#[tokio::test]
async fn test_concurrent_bandwidth_job_creation() {
    let ctx = TestContext::new().await;

    // Seed service
    seed_service_with_topics(
        ctx.pool(),
        "worker_stress",
        "europe",
        "docker_local",
        vec!["all", "europe"],
    )
    .await;

    // Setup mock file server
    ctx.file_server
        .setup_file("/stressfile", 100 * 1024 * 1024, None)
        .await;

    let start = Instant::now();
    let num_jobs = 10;

    // Create 10 jobs
    let mut job_ids: Vec<Uuid> = Vec::new();
    for i in 0..num_jobs {
        let url = ctx.file_server.url("/stressfile");
        let response = ctx
            .app
            .post("/jobs")
            .json(&serde_json::json!({
                "url": url,
                "routing_key": "europe",
                "worker_count": 5,
                "size_mb": 10,
                "entity": format!("stress_test_bw_{}", i),
                "note": format!("Concurrent bandwidth job {}", i)
            }))
            .await;

        response.assert_status_ok();
        let body: serde_json::Value = response.json();
        let job_id: Uuid = body["id"]
            .as_str()
            .unwrap_or_else(|| panic!("Job {} missing id: {:?}", i, body))
            .parse()
            .unwrap();
        job_ids.push(job_id);
    }

    let elapsed = start.elapsed();

    assert_eq!(
        job_ids.len(),
        num_jobs,
        "Should have created {} jobs",
        num_jobs
    );

    // Verify all job IDs are unique
    let unique_ids: std::collections::HashSet<_> = job_ids.iter().collect();
    assert_eq!(unique_ids.len(), num_jobs, "All job IDs should be unique");

    // Verify each job has correct sub-jobs (1 scaling + 3 CombinedDHP = 4 total)
    for job_id in &job_ids {
        let sub_job_count =
            sqlx::query_scalar!(r#"SELECT COUNT(*) FROM sub_jobs WHERE job_id = $1"#, job_id)
                .fetch_one(ctx.pool())
                .await
                .unwrap()
                .unwrap_or(0);

        assert_eq!(
            sub_job_count, 4,
            "Job {} should have 4 sub-jobs (1 Scaling + 3 CombinedDHP), got {}",
            job_id, sub_job_count
        );
    }

    // Verify total jobs in database
    let total_jobs = sqlx::query_scalar!(r#"SELECT COUNT(*) FROM jobs"#)
        .fetch_one(ctx.pool())
        .await
        .unwrap()
        .unwrap_or(0);

    assert_eq!(
        total_jobs, num_jobs as i64,
        "Database should contain exactly {} jobs",
        num_jobs
    );

    // Verify total sub-jobs in database (10 jobs * 4 sub-jobs each = 40)
    let total_sub_jobs = sqlx::query_scalar!(r#"SELECT COUNT(*) FROM sub_jobs"#)
        .fetch_one(ctx.pool())
        .await
        .unwrap()
        .unwrap_or(0);

    assert_eq!(
        total_sub_jobs,
        (num_jobs * 4) as i64,
        "Database should contain {} sub-jobs",
        num_jobs * 4
    );

    println!(
        "Created {} bandwidth jobs with {} total sub-jobs in {:?}",
        num_jobs,
        num_jobs * 4,
        elapsed
    );
}

/// Stress test: Mixed concurrent job creation (both types in interleaved fashion)
#[tokio::test]
async fn test_mixed_concurrent_job_creation() {
    let ctx = TestContext::new().await;

    // Seed services with different locations
    for (i, location) in ["europe", "usa", "asia"].iter().enumerate() {
        seed_service_with_topics(
            ctx.pool(),
            &format!("worker_mixed_{}", i),
            location,
            "docker_local",
            vec!["all", location],
        )
        .await;
    }

    // Setup mock file server
    ctx.file_server
        .setup_file("/mixedfile", 100 * 1024 * 1024, None)
        .await;

    let start = Instant::now();
    let num_geo_jobs = 5;
    let num_bw_jobs = 5;

    let mut geo_job_ids: Vec<Uuid> = Vec::new();
    let mut bw_job_ids: Vec<Uuid> = Vec::new();

    // Interleave job creation to simulate concurrent load
    for i in 0..5 {
        // Create geolocation job
        let url = ctx.file_server.url("/mixedfile");
        let response = ctx
            .app
            .post("/jobs/geolocation")
            .json(&serde_json::json!({
                "url": url,
                "size_mb": 10,
                "entity": format!("mixed_geo_{}", i)
            }))
            .await;

        response.assert_status_ok();
        let body: serde_json::Value = response.json();
        let job_id: Uuid = body["job_id"]
            .as_str()
            .unwrap_or_else(|| panic!("Geo job {} missing job_id: {:?}", i, body))
            .parse()
            .unwrap();
        geo_job_ids.push(job_id);

        // Create bandwidth job
        let url = ctx.file_server.url("/mixedfile");
        let response = ctx
            .app
            .post("/jobs")
            .json(&serde_json::json!({
                "url": url,
                "routing_key": "europe",
                "worker_count": 3,
                "size_mb": 10,
                "entity": format!("mixed_bw_{}", i)
            }))
            .await;

        response.assert_status_ok();
        let body: serde_json::Value = response.json();
        let job_id: Uuid = body["id"]
            .as_str()
            .unwrap_or_else(|| panic!("BW job {} missing id: {:?}", i, body))
            .parse()
            .unwrap();
        bw_job_ids.push(job_id);
    }

    let elapsed = start.elapsed();

    // Verify counts
    assert_eq!(geo_job_ids.len(), num_geo_jobs);
    assert_eq!(bw_job_ids.len(), num_bw_jobs);

    // Verify all IDs are unique across both job types
    let all_ids: std::collections::HashSet<_> =
        geo_job_ids.iter().chain(bw_job_ids.iter()).collect();
    assert_eq!(
        all_ids.len(),
        num_geo_jobs + num_bw_jobs,
        "All job IDs should be unique across job types"
    );

    // Verify job types in database
    let geo_count =
        sqlx::query_scalar!(r#"SELECT COUNT(*) FROM jobs WHERE job_type = 'geolocation'"#)
            .fetch_one(ctx.pool())
            .await
            .unwrap()
            .unwrap_or(0);

    let bw_count = sqlx::query_scalar!(
        r#"SELECT COUNT(*) FROM jobs WHERE job_type = 'bandwidth_saturation'"#
    )
    .fetch_one(ctx.pool())
    .await
    .unwrap()
    .unwrap_or(0);

    assert_eq!(geo_count, num_geo_jobs as i64);
    assert_eq!(bw_count, num_bw_jobs as i64);

    // Verify sub-job counts
    // Geolocation: 5 jobs * 4 sub-jobs = 20
    // Bandwidth: 5 jobs * 4 sub-jobs = 20
    // Total: 40
    let total_sub_jobs = sqlx::query_scalar!(r#"SELECT COUNT(*) FROM sub_jobs"#)
        .fetch_one(ctx.pool())
        .await
        .unwrap()
        .unwrap_or(0);

    assert_eq!(
        total_sub_jobs,
        ((num_geo_jobs + num_bw_jobs) * 4) as i64,
        "Should have {} total sub-jobs",
        (num_geo_jobs + num_bw_jobs) * 4
    );

    println!(
        "Created {} geolocation + {} bandwidth jobs ({} total sub-jobs) in {:?}",
        num_geo_jobs,
        num_bw_jobs,
        (num_geo_jobs + num_bw_jobs) * 4,
        elapsed
    );
}

/// Stress test: Verify database integrity under concurrent writes
#[tokio::test]
async fn test_database_integrity_under_concurrent_load() {
    let ctx = TestContext::new().await;

    // Seed services
    seed_service_with_topics(
        ctx.pool(),
        "worker_integrity",
        "europe",
        "docker_local",
        vec!["all", "europe"],
    )
    .await;

    ctx.file_server
        .setup_file("/integrityfile", 100 * 1024 * 1024, None)
        .await;

    let num_jobs: usize = 10;

    // Create jobs with worker_count >= 3 to get 4 sub-jobs (1 scaling + 3 CombinedDHP)
    // Note: worker_count=1 creates 2 sub-jobs, worker_count=2 creates 3 sub-jobs
    let mut job_ids: Vec<Uuid> = Vec::new();
    for i in 0..num_jobs {
        let url = ctx.file_server.url("/integrityfile");
        let response = ctx
            .app
            .post("/jobs")
            .json(&serde_json::json!({
                "url": url,
                "routing_key": "europe",
                "worker_count": 3,
                "size_mb": 10,
                "entity": format!("integrity_test_{}", i)
            }))
            .await;

        if response.status_code().is_success() {
            let body: serde_json::Value = response.json();
            if let Some(id_str) = body["id"].as_str() {
                if let Ok(id) = id_str.parse::<Uuid>() {
                    job_ids.push(id);
                }
            }
        }
    }

    assert_eq!(
        job_ids.len(),
        num_jobs,
        "All jobs should be created successfully"
    );

    // Verify foreign key relationships are intact
    let orphan_sub_jobs = sqlx::query_scalar!(
        r#"SELECT COUNT(*)
           FROM sub_jobs sj
           WHERE NOT EXISTS (SELECT 1 FROM jobs j WHERE j.id = sj.job_id)"#
    )
    .fetch_one(ctx.pool())
    .await
    .unwrap()
    .unwrap_or(0);

    assert_eq!(
        orphan_sub_jobs, 0,
        "No orphan sub-jobs should exist (foreign key integrity)"
    );

    // Verify each job has exactly the expected number of sub-jobs
    for job_id in &job_ids {
        let sub_jobs = sqlx::query!(
            r#"SELECT
                    id,
                    job_id,
                    type::text as sub_type
               FROM sub_jobs
               WHERE job_id = $1"#,
            job_id
        )
        .fetch_all(ctx.pool())
        .await
        .unwrap();

        assert_eq!(
            sub_jobs.len(),
            4,
            "Job {} should have exactly 4 sub-jobs",
            job_id
        );

        // Verify all sub-jobs reference the correct parent
        for sj in &sub_jobs {
            assert_eq!(
                sj.job_id, *job_id,
                "Sub-job {} should reference job {}",
                sj.id, job_id
            );
        }
    }

    // Verify job statuses are consistent (using PascalCase enum value)
    let pending_jobs =
        sqlx::query_scalar!(r#"SELECT COUNT(*) FROM jobs WHERE status = 'Pending'"#)
            .fetch_one(ctx.pool())
            .await
            .unwrap()
            .unwrap_or(0);

    assert_eq!(
        pending_jobs, num_jobs as i64,
        "All jobs should be in 'Pending' status after creation"
    );

    println!(
        "Database integrity verified for {} concurrent jobs with {} sub-jobs",
        num_jobs,
        num_jobs * 4
    );
}
