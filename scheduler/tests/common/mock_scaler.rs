// scheduler/tests/common/mock_scaler.rs

use async_trait::async_trait;
use scheduler::repository::service_repository::Service;
use scheduler::service_scaler::{ServiceScaler, ServiceScalerError, ServiceScalerInfo};
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Debug, Clone)]
pub struct ScaleCall {
    pub service_name: String,
    pub amount: i32,
}

pub struct MockServiceScaler {
    pub scale_up_calls: Arc<Mutex<Vec<ScaleCall>>>,
    pub scale_down_calls: Arc<Mutex<Vec<ScaleCall>>>,
}

impl MockServiceScaler {
    pub fn new() -> Self {
        Self {
            scale_up_calls: Arc::new(Mutex::new(Vec::new())),
            scale_down_calls: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub async fn get_scale_up_calls(&self) -> Vec<ScaleCall> {
        self.scale_up_calls.lock().await.clone()
    }

    pub async fn get_scale_down_calls(&self) -> Vec<ScaleCall> {
        self.scale_down_calls.lock().await.clone()
    }

    pub async fn clear_calls(&self) {
        self.scale_up_calls.lock().await.clear();
        self.scale_down_calls.lock().await.clear();
    }
}

impl Default for MockServiceScaler {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ServiceScaler for MockServiceScaler {
    async fn scale_up(&self, service: &Service, amount: i32) -> Result<(), ServiceScalerError> {
        self.scale_up_calls.lock().await.push(ScaleCall {
            service_name: service.name.clone(),
            amount,
        });
        Ok(())
    }

    async fn scale_down(&self, service: &Service, amount: i32) -> Result<(), ServiceScalerError> {
        self.scale_down_calls.lock().await.push(ScaleCall {
            service_name: service.name.clone(),
            amount,
        });
        Ok(())
    }

    async fn get_info(&self, service: &Service) -> Result<ServiceScalerInfo, ServiceScalerError> {
        Ok(ServiceScalerInfo::docker(service.name.clone(), 0))
    }
}
