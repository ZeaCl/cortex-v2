use async_trait::async_trait;

use super::error::{HealthStatus, WorkerError};
use crate::types::{ServiceRequest, ServiceResponse, StreamChunk, StreamResult};

#[async_trait]
pub trait Worker: Send + Sync {
    fn name(&self) -> &str;
    fn service_type(&self) -> &str;
    fn capabilities(&self) -> Vec<&str>;
    fn priority(&self) -> u8;

    async fn execute(&self, request: ServiceRequest) -> Result<ServiceResponse, WorkerError>;

    async fn execute_stream(&self, request: ServiceRequest) -> StreamResult {
        let response = self.execute(request).await?;
        let content = response.data.to_string();
        let stream = futures::stream::once(async move {
            Ok(StreamChunk { content, finish_reason: Some("stop".into()) })
        });
        Ok(Box::pin(stream))
    }

    async fn health_check(&self) -> Result<HealthStatus, WorkerError>;
    fn api_keys(&self) -> &[String];
    fn rotate_api_key(&mut self);
}
