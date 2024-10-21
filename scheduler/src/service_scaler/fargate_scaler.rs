use async_trait::async_trait;
use color_eyre::Result;

use super::{ServiceScaler, ServiceScalerError, ServiceScalerInfo};

pub struct FargateScaler;

#[async_trait]
impl ServiceScaler for FargateScaler {
    #[allow(unused_variables)]
    async fn scale_up(&self, service_name: String, amount: u64) -> Result<(), ServiceScalerError> {
        Err(ServiceScalerError::GenericError(
            "Not implemented".to_string(),
        ))
    }

    #[allow(unused_variables)]
    async fn scale_down(
        &self,
        service_name: String,
        amount: u64,
    ) -> Result<(), ServiceScalerError> {
        Err(ServiceScalerError::GenericError(
            "Not implemented".to_string(),
        ))
    }

    #[allow(unused_variables)]
    async fn get_info(
        &self,
        service_name: String,
    ) -> Result<ServiceScalerInfo, ServiceScalerError> {
        Err(ServiceScalerError::GenericError(
            "Not implemented".to_string(),
        ))
    }
}
