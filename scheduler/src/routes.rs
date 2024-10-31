use std::sync::Arc;

use axum::{
    extract::Request,
    middleware::{self, Next},
    response::Response,
    routing::{delete, get, post},
    Router,
};

use crate::{
    api::{
        api_response::{unauthorized, ApiResponse},
        create_job, get_data, healthcheck, services,
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
        .route("/data", get(get_data::handle))
        .route("/job", post(create_job::handle));

    let auth_routes = Router::new()
        .route("/services", get(services::get_services::handle))
        .route("/services", post(services::create_service::handle))
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
        .layer(middleware::from_fn(auth));

    routes.merge(auth_routes)
}
