use std::{process::Command, str};

use async_trait::async_trait;
use color_eyre::Result;

use crate::service_repository::Service;

use super::{ServiceScaler, ServiceScalerError, ServiceScalerInfo};

pub struct DockerScaler;

#[async_trait]
impl ServiceScaler for DockerScaler {
    async fn scale_up(&self, service: &Service, amount: i32) -> Result<(), ServiceScalerError> {
        let current_count = self.get_instance_count(&service.name)?;
        let new_count = current_count + amount.try_into().unwrap_or(0);

        if current_count == new_count {
            return Ok(());
        }

        self.scale_service(&service.name, new_count)?;

        Ok(())
    }

    async fn scale_down(&self, service: &Service, amount: i32) -> Result<(), ServiceScalerError> {
        let current_count = self.get_instance_count(&service.name)?;
        let new_count = current_count
            .saturating_sub(amount.try_into().unwrap_or(u64::MAX))
            .max(0);

        if current_count == new_count {
            return Ok(());
        }

        self.scale_service(&service.name, new_count)?;

        Ok(())
    }

    async fn get_info(&self, service: &Service) -> Result<ServiceScalerInfo, ServiceScalerError> {
        let instances = self.get_instance_count(&service.name)?;

        Ok(ServiceScalerInfo::docker(service.name.clone(), instances))
    }
}

impl DockerScaler {
    pub fn new() -> Self {
        Self {}
    }

    fn get_instance_count(&self, service_name: &str) -> Result<u64, ServiceScalerError> {
        let output = Command::new("docker")
            .args([
                "ps",
                "--filter",
                &format!("label=com.docker.compose.service={}", service_name),
                "--format",
                "{{.ID}}",
            ])
            .output()
            .map_err(|e| ServiceScalerError::CommandError(e.to_string()))?;

        if !output.status.success() {
            return Err(ServiceScalerError::CommandError(
                String::from_utf8_lossy(&output.stderr).to_string(),
            ));
        }

        let output_str = String::from_utf8_lossy(&output.stdout);
        let count = output_str.lines().count() as u64;

        Ok(count)
    }

    fn scale_service(&self, service_name: &str, count: u64) -> Result<(), ServiceScalerError> {
        Command::new("docker")
            .args([
                "compose",
                "up",
                "-d",
                "--scale",
                &format!("{}={}", service_name, count),
                service_name,
            ])
            .status()
            .map_err(|e| ServiceScalerError::CommandError(e.to_string()))?;

        Ok(())
    }
}
