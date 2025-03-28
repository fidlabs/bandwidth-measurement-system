use common::api_response;
use utoipa::openapi::security::{HttpAuthScheme, HttpBuilder, SecurityScheme};
use utoipa::{Modify, OpenApi};

use crate::{
    api::{
        healthcheck,
        jobs::{cancel_job, create_job, get_job, get_jobs},
        services::{
            create_service, delete_service, get_services, services_info, services_scale_down,
            services_scale_down_all, services_scale_up, update_service,
        },
    },
    job_repository, service_repository, service_scaler, sub_job_repository,
};

// SecurityAddon struct to add security schemes
struct SecurityAddon;
impl Modify for SecurityAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        if let Some(components) = openapi.components.as_mut() {
            let scheme =
                SecurityScheme::Http(HttpBuilder::new().scheme(HttpAuthScheme::Bearer).build());
            components.add_security_scheme("bearer_auth", scheme);
        }
    }
}

#[derive(OpenApi)]
#[openapi(
    // API Metadata
    info(
        title = "BMS: Bandwidth Management System",
        description = r#"
This is the API documentation for the Bandwidth Management System (BMS) service.

Creating a Job requires an endpoint URL to a file (e.g. piece) as well as a routing_key (service topic) to which the job will be sent.
        "#,
        version = "1.0.0"
    ),
    // API Handler Functions
    paths(
        // Healthcheck
        healthcheck::handle_healthcheck,
        // Jobs
        create_job::handle_create_job,
        cancel_job::handle_cancel_job,
        get_jobs::handle_get_jobs,
        get_job::handle_get_job,
        // Services
        create_service::handle_create_service,
        delete_service::handle_delete_service,
        get_services::handle_get_services,
        update_service::handle_update_service,
        services_info::handle_services_info,
        services_scale_up::handle_services_scale_up,
        services_scale_down::handle_services_scale_down,
        services_scale_down_all::handle_services_scale_down_all
    ),
    components(
        schemas(
            // Jobs Schemas
            cancel_job::CancelJobPathParams,
            cancel_job::CancelJobResponse,

            create_job::CreateJobInput,
            create_job::CreateJobResponse,

            get_jobs::GetJobsQueryParams,
            get_jobs::GetJobsResponse,

            get_job::GetJobPathParams,
            get_job::GetJobResponse,
            get_job::JobSummary,

            // Services Schemas
            create_service::CreateServiceInput,
            create_service::CreateServiceResponse,

            delete_service::DeleteServicePathInput,
            delete_service::DeleteServiceResponse,

            get_services::GetServicesResponse,
            get_services::ServiceWithTopicsWithInfo,

            services_info::ServicesScaleInfoPathInput,
            services_info::ServicesScaleInfoResponse,

            update_service::UpdateServicePathInput,
            update_service::UpdateServiceInput,
            update_service::UpdateServiceResponse,

            services_scale_up::ServicesScaleUpPathInput,
            services_scale_up::ServicesScaleUpInput,
            services_scale_up::ServiceScaleUpResponse,

            services_scale_down::ServicesScaleDownPathInput,
            services_scale_down::ServicesScaleDownInput,
            services_scale_down::ServiceScaleDownResponse,

            services_scale_down_all::ServiceScaleDownAllResponse,
            services_scale_down_all::ServiceWithInfo,

            healthcheck::HealthcheckResponse,

            // Common Schemas
            api_response::ErrorResponse,

            // Additional Schemas
            job_repository::Job,
            job_repository::JobStatus,
            job_repository::JobWithSubJobsWithData,
            job_repository::SubJobWithData,
            job_repository::WorkerData,
            job_repository::JobDetails,
            job_repository::JobWithSubJobs,

            service_repository::Service,
            service_repository::ServiceWithTopics,

            sub_job_repository::SubJob,
            sub_job_repository::SubJobType,
            sub_job_repository::SubJobStatus,

            service_scaler::ServiceScalerInfo,
        ),
      ),
    modifiers(&SecurityAddon),
    tags(
        // API Categories
        (name = "Healthcheck", description = "Healthcheck API"),
        (name = "Jobs", description = "Job management APIs"),
        (name = "Services", description = "Service management APIs"),
    )
)]
pub struct ApiDoc;
