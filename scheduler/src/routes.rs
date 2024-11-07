use std::sync::Arc;

use axum::{
    extract::Request,
    middleware::{self, Next},
    response::Response,
    routing::{delete, get, post, put},
    Router,
};

use crate::{
    api::{
        api_response::{unauthorized, ApiResponse},
        healthcheck, jobs, services,
    },
    config::CONFIG,
    state::AppState,
};

async fn auth(req: Request, next: Next) -> Result<Response, ApiResponse<()>> {
    if let Some(auth_header) = req.headers().get("Authorization") {
        // simple auth validation
        if auth_header.to_str().is_ok()
            && auth_header.to_str().unwrap() == format!("Bearer {}", &CONFIG.auth_token)
        {
            let response = next.run(req).await;
            return Ok(response);
        }
    }
    Err(unauthorized("Unauthorized"))
}

pub fn create_routes() -> Router<Arc<AppState>> {
    let routes = Router::new()
        .route("/healthcheck", get(healthcheck::handle))
        .route("/jobs", post(jobs::create_job::handle))
        .route("/jobs", get(jobs::get_jobs::handle))
        .route("/jobs/:job_id", get(jobs::get_job::handle))
        .route("/jobs/:job_id", delete(jobs::cancel_job::handle));

    let auth_routes = Router::new()
        .route("/services", get(services::get_services::handle))
        .route("/services", post(services::create_service::handle))
        .route("/services", put(services::update_service::handle))
        .route("/services", delete(services::delete_service::handle))
        .route("/services/scale/info", get(services::services_info::handle))
        .route(
            "/services/scale/up",
            post(services::services_scale_up::handle),
        )
        .route(
            "/services/scale/down",
            post(services::services_scale_down::handle),
        )
        .route(
            "/services/scale/down/all",
            post(services::services_scale_down_all::handle),
        )
        .layer(middleware::from_fn(auth));

    routes.merge(auth_routes)
}
