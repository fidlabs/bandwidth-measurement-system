// Library exports for scheduler - allows tests and other binaries to use internal types

pub mod background;
pub mod config;
pub mod repository;
pub mod routes;
pub mod service_scaler;
pub mod state;
pub mod url_validator;

// Internal modules (not public)
mod api;
mod queue;
mod types;

// Re-export types from private modules that main.rs needs
pub use api::api_doc::ApiDoc;
pub use queue::data_consumer::DataConsumer;
pub use queue::status_consumer::StatusConsumer;

// Re-export repository modules at crate root for backward compatibility
// This allows code to use `crate::job_repository` instead of `crate::repository::job_repository`
pub use repository::data_repository;
pub use repository::geolocation_repository;
pub use repository::job_repository;
pub use repository::service_repository;
pub use repository::sub_job_repository;
pub use repository::topic_repository;
pub use repository::worker_repository;

// Re-export commonly used types for convenience
pub use repository::service_repository::{ProviderType, Service};
pub use repository::Repositories;
pub use service_scaler::{ServiceScaler, ServiceScalerRegistry};
pub use state::AppState;
