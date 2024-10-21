pub mod docker_scaler;
pub mod fargate_scaler;

use async_trait::async_trait;

#[derive(Debug)]
#[allow(dead_code)]
pub enum ServiceScalerError {
    CommandError(String),
    GenericError(String),
}

#[derive(Debug)]
pub struct ServiceScalerInfo {
    pub name: String,
    pub instances: u64,
}

#[async_trait]
pub trait ServiceScaler: Send + Sync {
    async fn scale_up(&self, service_name: String, amount: u64) -> Result<(), ServiceScalerError>;

    async fn scale_down(&self, service_name: String, amount: u64)
        -> Result<(), ServiceScalerError>;

    async fn get_info(&self, service_name: String)
        -> Result<ServiceScalerInfo, ServiceScalerError>;
}
