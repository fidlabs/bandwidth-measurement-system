pub mod docker_scaler;
pub mod fargate_scaler;

use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;
use color_eyre::Result;
use docker_scaler::DockerScaler;
use fargate_scaler::FargateScaler;
use serde::Serialize;
use utoipa::ToSchema;

use crate::{
    config::CONFIG,
    service_repository::{ProviderType, Service},
};

#[derive(Debug)]
#[allow(dead_code)]
pub enum ServiceScalerError {
    CommandError(String),
    GenericError(String),
    InvalidService(String),
}
impl ServiceScalerError {
    pub fn to_str(&self) -> String {
        match self {
            ServiceScalerError::CommandError(msg) => {
                format!("ServiceScalerError::CommandError: {msg}")
            }
            ServiceScalerError::GenericError(msg) => {
                format!("ServiceScalerError::GenericError: {msg}")
            }
            ServiceScalerError::InvalidService(msg) => {
                format!("ServiceScalerError::InvalidService: {msg}")
            }
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
#[allow(dead_code)]
pub struct ServiceScalerInfo {
    pub name: String,
    pub instances: u64,
    pub provider_type: ProviderType,
    pub desired_count: Option<i32>,
    pub running_count: Option<i32>,
    pub pending_count: Option<i32>,
}

impl ServiceScalerInfo {
    pub fn docker(name: String, instances: u64) -> Self {
        Self {
            name,
            instances,
            provider_type: ProviderType::DockerLocal,
            desired_count: None,
            running_count: None,
            pending_count: None,
        }
    }
    pub fn fargate(
        name: String,
        instances: u64,
        desired_count: i32,
        running_count: i32,
        pending_count: i32,
    ) -> Self {
        Self {
            name,
            instances,
            provider_type: ProviderType::AWSFargate,
            desired_count: Some(desired_count),
            running_count: Some(running_count),
            pending_count: Some(pending_count),
        }
    }
}

#[async_trait]
pub trait ServiceScaler: Send + Sync {
    async fn scale_up(&self, service: &Service, amount: i32) -> Result<(), ServiceScalerError>;

    async fn scale_down(&self, service: &Service, amount: i32) -> Result<(), ServiceScalerError>;

    async fn get_info(&self, service: &Service) -> Result<ServiceScalerInfo, ServiceScalerError>;
}

pub struct ServiceScalerRegistry {
    scalers: HashMap<ProviderType, Arc<dyn ServiceScaler>>,
}

impl ServiceScalerRegistry {
    pub fn new() -> Self {
        let mut scalers: HashMap<ProviderType, Arc<dyn ServiceScaler>> = HashMap::new();

        // DockerLocal should only be available in local mode
        if CONFIG.local_mode == "true" {
            scalers.insert(ProviderType::DockerLocal, Arc::new(DockerScaler::new()));
        } else {
            scalers.insert(ProviderType::AWSFargate, Arc::new(FargateScaler::new()));
        }

        Self { scalers }
    }

    pub fn get_scaler(&self, provider_type: &ProviderType) -> Option<&Arc<dyn ServiceScaler>> {
        self.scalers.get(provider_type)
    }
}
