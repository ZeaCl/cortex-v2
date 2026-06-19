use crate::registry::WorkerRegistry;
use crate::router::pipeline::{Step, StepResult};
use crate::types::ServiceRequest;
use crate::worker::error::WorkerError;
use async_trait::async_trait;

/// Paso del pipeline: intenta ejecutar el request en cada worker del registro.
/// Si uno falla, prueba el siguiente. Si todos fallan, devuelve error.
pub struct FailoverStep {
    registry: WorkerRegistry,
    service_type: String,
}

impl FailoverStep {
    pub fn new(registry: WorkerRegistry, service_type: String) -> Self {
        Self { registry, service_type }
    }
}

#[async_trait]
impl Step for FailoverStep {
    async fn handle(&self, request: &ServiceRequest) -> Result<StepResult, WorkerError> {
        let names = self.registry.list_by_service_type(&self.service_type);
        if names.is_empty() {
            return Err(WorkerError::ConfigError("no workers available".into()));
        }

        for name in &names {
            if let Some(worker) = self.registry.get(name) {
                match worker.execute(request.clone()).await {
                    Ok(response) => return Ok(StepResult::Done(response)),
                    Err(_) => continue,
                }
            }
        }

        Err(WorkerError::ConfigError("all workers failed".into()))
    }
}
