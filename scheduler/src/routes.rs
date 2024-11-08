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
        .route("/healthcheck", get(healthcheck::handle_healthcheck))
        .route("/jobs", post(jobs::create_job::handle_create_job))
        .route("/jobs", get(jobs::get_jobs::handle_get_jobs))
        .route("/jobs/:job_id", get(jobs::get_job::handle_get_job))
        .route("/jobs/:job_id", delete(jobs::cancel_job::handle_cancel_job));

    let auth_routes = Router::new()
        .route(
            "/services",
            get(services::get_services::handle_get_services),
        )
        .route(
            "/services",
            post(services::create_service::handle_create_service),
        )
        .route(
            "/services/scale/down/all",
            post(services::services_scale_down_all::handle_services_scale_down_all),
        )
        .route(
            "/services/:service_id",
            put(services::update_service::handle_update_service),
        )
        .route(
            "/services/:service_id",
            delete(services::delete_service::handle_delete_service),
        )
        .route(
            "/services/:service_id/scale/info",
            get(services::services_info::handle_services_info),
        )
        .route(
            "/services/:service_id/scale/up",
            post(services::services_scale_up::handle_services_scale_up),
        )
        .route(
            "/services/:service_id/scale/down",
            post(services::services_scale_down::handle_services_scale_down),
        )
        .layer(middleware::from_fn(auth));

    routes.merge(auth_routes)
}
