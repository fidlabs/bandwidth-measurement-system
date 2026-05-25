use color_eyre::Result;
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use utoipa::ToSchema;
use uuid::Uuid;

use super::sub_job_repository::SubJobType;

#[derive(Clone)]
pub struct GeolocationRepository {
    pool: PgPool,
}

#[derive(Debug, Serialize, Deserialize, ToSchema, Clone)]
pub struct LocationResult {
    pub location: String,
    pub status: LocationStatus,
    pub ttfb_ms: Option<f64>,
    pub bandwidth_mbps: Option<f64>,
    pub worker_count: Option<i64>,
    pub error: Option<String>,
    pub sub_job_id: Uuid,
}

#[derive(Debug, Serialize, Deserialize, ToSchema, Clone, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum LocationStatus {
    Completed,
    Failed,
    Canceled,
    Processing,
    Pending,
}

impl From<Option<&str>> for LocationStatus {
    fn from(s: Option<&str>) -> Self {
        match s {
            Some("Completed") => Self::Completed,
            Some("Failed") => Self::Failed,
            Some("Canceled") => Self::Canceled,
            Some("Processing") => Self::Processing,
            Some("Pending") | Some("Created") => Self::Pending,
            _ => Self::Pending,
        }
    }
}

impl GeolocationRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn get_location_results(
        &self,
        job_id: Uuid,
    ) -> Result<Vec<LocationResult>, sqlx::Error> {
        // Get sub-job status for each location
        let sub_jobs = sqlx::query!(
            r#"
            SELECT
                id,
                details->>'topic' as location,
                status::text as status,
                details->>'error' as error
            FROM sub_jobs
            WHERE job_id = $1 AND type = $2
            "#,
            job_id,
            SubJobType::CombinedDHP as SubJobType
        )
        .fetch_all(&self.pool)
        .await?;

        let mut results = Vec::new();

        for sub_job in sub_jobs {
            let location = sub_job.location.unwrap_or_else(|| "unknown".to_string());
            let sub_job_id = sub_job.id;

            // Map sub-job status to location status (database stores PascalCase enum values)
            let location_status: LocationStatus = sub_job.status.as_deref().into();

            // If completed, get aggregated metrics
            let (ttfb_ms, bandwidth_mbps, worker_count) =
                if location_status == LocationStatus::Completed {
                    let metrics = sqlx::query(
                        r#"
                    SELECT
                        AVG((d.download->>'time_to_first_byte_ms')::float) as avg_ttfb,
                        AVG((d.download->>'download_speed')::float) as avg_bandwidth,
                        COUNT(DISTINCT d.worker_name) as worker_count
                    FROM worker_data d
                    WHERE d.sub_job_id = $1
                      AND d.is_success = TRUE
                    "#,
                    )
                    .bind(sub_job_id)
                    .fetch_one(&self.pool)
                    .await
                    .ok();

                    if let Some(m) = metrics {
                        (
                            m.try_get("avg_ttfb").ok(),
                            m.try_get("avg_bandwidth").ok(),
                            m.try_get("worker_count").ok(),
                        )
                    } else {
                        (None, None, None)
                    }
                } else {
                    (None, None, None)
                };

            let error = if location_status == LocationStatus::Failed
                || location_status == LocationStatus::Canceled
            {
                sub_job.error
            } else {
                None
            };

            results.push(LocationResult {
                location,
                status: location_status,
                ttfb_ms,
                bandwidth_mbps,
                worker_count,
                error,
                sub_job_id,
            });
        }

        // Sort by ttfb_ms (lowest first), putting None values last
        results.sort_by(|a, b| match (a.ttfb_ms, b.ttfb_ms) {
            (Some(a_ttfb), Some(b_ttfb)) => a_ttfb.total_cmp(&b_ttfb),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => std::cmp::Ordering::Equal,
        });

        Ok(results)
    }

    /// Calculate the closest location based on lowest TTFB among completed results.
    /// Only considers locations with status=Completed and valid ttfb_ms.
    /// Ignores Failed, Canceled, Processing, and Pending locations.
    pub fn calculate_closest_location(location_results: &[LocationResult]) -> Option<String> {
        location_results
            .iter()
            .filter(|r| r.status == LocationStatus::Completed && r.ttfb_ms.is_some())
            .min_by(|a, b| a.ttfb_ms.unwrap().total_cmp(&b.ttfb_ms.unwrap()))
            .map(|r| r.location.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_result(location: &str, status: LocationStatus, ttfb_ms: Option<f64>) -> LocationResult {
        LocationResult {
            location: location.to_string(),
            status,
            ttfb_ms,
            bandwidth_mbps: None,
            worker_count: None,
            error: None,
            sub_job_id: Uuid::new_v4(),
        }
    }

    #[test]
    fn test_calculate_closest_location_returns_lowest_ttfb() {
        let results = vec![
            make_result("usa", LocationStatus::Completed, Some(120.0)),
            make_result("europe", LocationStatus::Completed, Some(45.0)),
            make_result("asia", LocationStatus::Completed, Some(200.0)),
        ];

        let closest = GeolocationRepository::calculate_closest_location(&results);
        assert_eq!(closest, Some("europe".to_string()));
    }

    #[test]
    fn test_calculate_closest_location_ignores_failed() {
        let results = vec![
            make_result("europe", LocationStatus::Failed, Some(10.0)), // lowest but failed
            make_result("usa", LocationStatus::Completed, Some(120.0)),
            make_result("asia", LocationStatus::Completed, Some(200.0)),
        ];

        let closest = GeolocationRepository::calculate_closest_location(&results);
        assert_eq!(closest, Some("usa".to_string()));
    }

    #[test]
    fn test_calculate_closest_location_ignores_none_ttfb() {
        let results = vec![
            make_result("europe", LocationStatus::Completed, None), // completed but no ttfb
            make_result("usa", LocationStatus::Completed, Some(120.0)),
        ];

        let closest = GeolocationRepository::calculate_closest_location(&results);
        assert_eq!(closest, Some("usa".to_string()));
    }

    #[test]
    fn test_calculate_closest_location_returns_none_when_no_completed() {
        let results = vec![
            make_result("europe", LocationStatus::Failed, Some(10.0)),
            make_result("usa", LocationStatus::Processing, Some(20.0)),
            make_result("asia", LocationStatus::Pending, None),
        ];

        let closest = GeolocationRepository::calculate_closest_location(&results);
        assert_eq!(closest, None);
    }

    #[test]
    fn test_calculate_closest_location_empty_list() {
        let results: Vec<LocationResult> = vec![];
        let closest = GeolocationRepository::calculate_closest_location(&results);
        assert_eq!(closest, None);
    }

    #[test]
    fn test_calculate_closest_location_single_completed() {
        let results = vec![make_result("europe", LocationStatus::Completed, Some(50.0))];
        let closest = GeolocationRepository::calculate_closest_location(&results);
        assert_eq!(closest, Some("europe".to_string()));
    }

    #[test]
    fn test_calculate_closest_location_ignores_canceled() {
        let results = vec![
            make_result("europe", LocationStatus::Canceled, Some(10.0)), // lowest but canceled
            make_result("usa", LocationStatus::Completed, Some(120.0)),
            make_result("asia", LocationStatus::Completed, Some(200.0)),
        ];

        let closest = GeolocationRepository::calculate_closest_location(&results);
        assert_eq!(closest, Some("usa".to_string()));
    }

    #[test]
    fn test_calculate_closest_location_mixed_terminal_states() {
        let results = vec![
            make_result("europe", LocationStatus::Canceled, Some(10.0)),
            make_result("usa", LocationStatus::Failed, Some(20.0)),
            make_result("asia", LocationStatus::Completed, Some(200.0)),
        ];

        let closest = GeolocationRepository::calculate_closest_location(&results);
        assert_eq!(closest, Some("asia".to_string()));
    }
}
