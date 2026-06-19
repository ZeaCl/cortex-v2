use crate::types::{ServiceRequest, ServiceResponse};
use crate::worker::error::WorkerError;
use async_trait::async_trait;
use std::future::Future;
use std::pin::Pin;

/// Resultado de un paso del pipeline.
pub enum StepResult {
    Done(ServiceResponse),
    Next,
}

/// Un paso del pipeline. Decide si maneja el request o lo pasa.
#[async_trait]
pub trait Step: Send + Sync {
    async fn handle(&self, request: &ServiceRequest) -> Result<StepResult, WorkerError>;
}

/// Una cadena de pasos que se ejecutan en orden hasta que uno responde.
pub struct Pipeline {
    pub steps: Vec<Box<dyn Step>>,
}

impl Pipeline {
    pub fn new(steps: Vec<Box<dyn Step>>) -> Self {
        Self { steps }
    }

    /// Ejecuta la cadena recursivamente.
    /// step[0] decide → si Next, llama a step[1], y así.
    pub async fn run(&self, request: ServiceRequest) -> Result<ServiceResponse, WorkerError> {
        self.run_from(0, request).await
    }

    fn run_from(
        &self,
        index: usize,
        request: ServiceRequest,
    ) -> Pin<Box<dyn Future<Output = Result<ServiceResponse, WorkerError>> + Send + '_>> {
        if index >= self.steps.len() {
            return Box::pin(async { Err(WorkerError::ConfigError("pipeline exhausted".into())) });
        }

        let step = &self.steps[index];
        let next_index = index + 1;

        Box::pin(async move {
            match step.handle(&request).await? {
                StepResult::Done(response) => Ok(response),
                StepResult::Next => self.run_from(next_index, request).await,
            }
        })
    }
}
