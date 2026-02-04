// scheduler/tests/integration/smoke_test.rs
// Simple smoke test to verify TestContext setup works

use crate::common::TestContext;

#[tokio::test]
async fn test_context_creation() {
    let _ctx = TestContext::new().await;
    // If we get here, TestContext was created successfully
}

#[tokio::test]
async fn test_healthcheck_endpoint() {
    let ctx = TestContext::new().await;

    let response = ctx.app.get("/healthcheck").await;

    response.assert_status_ok();

    let json: serde_json::Value = response.json();
    assert_eq!(json["status"], "ok");
}
