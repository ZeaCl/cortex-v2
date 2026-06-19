use crate::types::{ServiceRequest, ServiceResponse};
use crate::worker::error::WorkerError;
use async_trait::async_trait;

/// Resultado de un paso del pipeline: o manejó el request, o lo pasa al siguiente.
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

    /// Ejecuta la cadena. Cada paso decide: ¿respondo yo, o paso al siguiente?
    pub async fn run(&self, request: ServiceRequest) -> Result<ServiceResponse, WorkerError> {
        for step in &self.steps {
            match step.handle(&request).await? {
                StepResult::Done(response) => return Ok(response),
                StepResult::Next => continue,
            }
        }
        Err(WorkerError::ConfigError("pipeline exhausted".into()))
    }
}
