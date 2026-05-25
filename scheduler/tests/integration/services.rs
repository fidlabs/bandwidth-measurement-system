use crate::common::*;
use sqlx::Row;

#[tokio::test]
async fn test_update_service_sets_location_and_replaces_topics() {
    let ctx = TestContext::new().await;

    let service_id = seed_service_with_topics(
        ctx.pool(),
        "worker_eu_pl",
        "europe",
        "docker_local",
        vec!["all", "europe"],
    )
    .await;

    let response = ctx
        .app
        .put(&format!("/services/{service_id}"))
        .authorization_bearer("mysecrettokenthatdefinatelyisnotongithubpublicrepo")
        .json(&serde_json::json!({
            "is_enabled": true,
            "location": "poland",
            "topics": ["all", "europe", "poland"]
        }))
        .await;

    response.assert_status_ok();

    let service = sqlx::query(
        r#"
        SELECT location
        FROM services
        WHERE id = $1
        "#,
    )
    .bind(service_id)
    .fetch_one(ctx.pool())
    .await
    .unwrap();

    assert_eq!(
        service.get::<Option<String>, _>("location"),
        Some("poland".to_string())
    );

    let topics = sqlx::query_scalar::<_, String>(
        r#"
        SELECT t.name
        FROM service_topics st
        JOIN topics t ON t.id = st.topic_id
        WHERE st.service_id = $1
        ORDER BY t.name
        "#,
    )
    .bind(service_id)
    .fetch_all(ctx.pool())
    .await
    .unwrap();

    assert_eq!(topics, vec!["all", "europe", "poland"]);
}
