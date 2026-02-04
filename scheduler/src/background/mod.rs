pub mod service_descaler;
pub mod sub_job_handler;
pub mod worker_online_check;

mod sub_job_combineddhp;
mod sub_job_scaling;

// Re-export for testing
pub use sub_job_handler::process_pending_sub_jobs;
