use async_trait::async_trait;
use aws_config::Region;
use aws_sdk_ecs::Client;
use color_eyre::Result;
use tracing::{debug, error};

use crate::service_repository::Service;

use super::{ServiceScaler, ServiceScalerError, ServiceScalerInfo};

pub struct FargateScaler {}

#[async_trait]
impl ServiceScaler for FargateScaler {
    async fn scale_up(&self, service: &Service, amount: i32) -> Result<(), ServiceScalerError> {
        let service_info = self.get_service_info(service).await?;
        let new_count = service_info.desired_count.unwrap() + amount;

        if service_info.desired_count.unwrap_or(0) == new_count {
            return Ok(());
        }

        self.update_service(service, new_count).await?;

        Ok(())
    }

    async fn scale_down(&self, service: &Service, amount: i32) -> Result<(), ServiceScalerError> {
        let service_info = self.get_service_info(service).await?;
        let new_count = service_info
            .desired_count
            .unwrap()
            .saturating_sub(amount)
            .max(0);

        tracing::error!(
            "desired_count: {}, new_count: {}, amount: {}",
            service_info.desired_count.unwrap_or(0),
            new_count,
            amount
        );

        if service_info.desired_count.unwrap_or(0) == new_count {
            return Ok(());
        }

        self.update_service(service, new_count).await?;

        Ok(())
    }

    async fn get_info(&self, service: &Service) -> Result<ServiceScalerInfo, ServiceScalerError> {
        self.get_service_info(service).await
    }
}

impl FargateScaler {
    pub fn new() -> Self {
        Self {}
    }

    async fn build_client(&self, region_name: String) -> Client {
        let config = aws_config::from_env()
            .region(Region::new(region_name))
            .load()
            .await;

        Client::new(&config)
    }

    async fn get_service_info(
        &self,
        service: &Service,
    ) -> Result<ServiceScalerInfo, ServiceScalerError> {
        let region_name = service.details["region"]
            .as_str()
            .ok_or(ServiceScalerError::InvalidService(
                "Region not found".to_string(),
            ))?
            .to_string();

        let cluster =
            service.details["cluster"]
                .as_str()
                .ok_or(ServiceScalerError::InvalidService(
                    "Cluster not found".to_string(),
                ))?;

        let resp = self
            .build_client(region_name)
            .await
            .describe_services()
            .cluster(cluster)
            .services(&service.name)
            .send()
            .await
            .map_err(|e| {
                error!("DescribeServices failed: {:?}", e);
                ServiceScalerError::GenericError(format!("DescribeServices failed: {}", e))
            })?;

        debug!("DescribeServices response: {:?}", resp);

        let aws_service = resp
            .services()
            .first()
            .ok_or_else(|| ServiceScalerError::GenericError("Service not found".to_string()))?;

        debug!("Service fargate describe_services: {:?}", aws_service);

        Ok(ServiceScalerInfo::fargate(
            service.name.clone(),
            aws_service.running_count() as u64,
            aws_service.desired_count(),
            aws_service.running_count(),
            aws_service.pending_count(),
        ))
    }

    async fn update_service(
        &self,
        service: &Service,
        desired_count: i32,
    ) -> Result<(), ServiceScalerError> {
        let region_name = service.details["region"]
            .as_str()
            .ok_or(ServiceScalerError::InvalidService(
                "Region not found".to_string(),
            ))?
            .to_string();

        let cluster =
            service.details["cluster"]
                .as_str()
                .ok_or(ServiceScalerError::InvalidService(
                    "Cluster not found".to_string(),
                ))?;

        let res = self
            .build_client(region_name)
            .await
            .update_service()
            .cluster(cluster)
            .service(&service.name)
            .desired_count(desired_count)
            .send()
            .await
            .map_err(|e| {
                error!("UpdateService failed: {:?}", e);
                ServiceScalerError::GenericError(format!("UpdateService failed: {}", e))
            })?;

        let aws_service = res
            .service()
            .ok_or_else(|| ServiceScalerError::GenericError("Service not found".to_string()))?;

        debug!("Service fargate update_service: {:?}", aws_service);

        Ok(())
    }
}
