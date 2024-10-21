use std::{process::Command, str};

use async_trait::async_trait;
use color_eyre::Result;

use super::{ServiceScaler, ServiceScalerError, ServiceScalerInfo};

pub struct DockerScaler;

#[async_trait]
impl ServiceScaler for DockerScaler {
    async fn scale_up(&self, service_name: String, amount: u64) -> Result<(), ServiceScalerError> {
        let current_count = self.get_instance_count(&service_name)?;
        let new_count = current_count + amount;

        self.scale_service(&service_name, new_count)?;
        Ok(())
    }

    async fn scale_down(
        &self,
        service_name: String,
        amount: u64,
    ) -> Result<(), ServiceScalerError> {
        let current_count = self.get_instance_count(&service_name)?;
        let new_count = current_count.saturating_sub(amount);

        self.scale_service(&service_name, new_count)?;
        Ok(())
    }

    async fn get_info(
        &self,
        service_name: String,
    ) -> Result<ServiceScalerInfo, ServiceScalerError> {
        let instances = self.get_instance_count(&service_name)?;
        Ok(ServiceScalerInfo {
            name: service_name.clone(),
            instances,
        })
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
