pub mod pipeline;
pub mod steps;

use crate::types::{ServiceRequest, ServiceResponse};
use crate::worker::error::WorkerError;
use crate::worker::registry::WorkerRegistry;
use pipeline::{Pipeline, Step};
use steps::failover::FailoverStep;

/// El Router: un Pipeline con pasos encadenados.
/// Por defecto contiene solo FailoverStep. Se pueden agregar más con with_step().
pub struct Router {
    pipeline: Pipeline,
}

impl Router {
    pub fn new(registry: WorkerRegistry, service_type: &str) -> Self {
        let steps: Vec<Box<dyn Step>> =
            vec![Box::new(FailoverStep::new(registry, service_type.to_string()))];
        Self { pipeline: Pipeline::new(steps) }
    }

    /// Agrega un paso antes del failover (rate limit, circuit breaker, etc).
    pub fn with_step(mut self, step: Box<dyn Step>) -> Self {
        self.pipeline.steps.insert(self.pipeline.steps.len() - 1, step);
        self
    }

    pub async fn route(&self, request: ServiceRequest) -> Result<ServiceResponse, WorkerError> {
        self.pipeline.run(request).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::router::pipeline::{Step, StepResult};
    use crate::types::UserContext;
    use crate::worker::Worker;
    use crate::worker::error::HealthStatus;
    use async_trait::async_trait;
    use std::collections::HashMap;

    struct PassStep;

    #[async_trait]
    impl Step for PassStep {
        async fn handle(&self, _request: &ServiceRequest) -> Result<StepResult, WorkerError> {
            Ok(StepResult::Next)
        }
    }

    struct BlockStep {
        status: u16,
    }

    #[async_trait]
    impl Step for BlockStep {
        async fn handle(&self, _request: &ServiceRequest) -> Result<StepResult, WorkerError> {
            Err(WorkerError::HttpError { status: self.status, message: "blocked".into() })
        }
    }

    struct OkWorker {
        name: String,
    }

    #[async_trait]
    impl Worker for OkWorker {
        fn name(&self) -> &str {
            &self.name
        }
        fn service_type(&self) -> &str {
            "llm"
        }
        fn capabilities(&self) -> Vec<&str> {
            vec!["chat"]
        }
        fn priority(&self) -> u8 {
            10
        }
        async fn execute(&self, _: ServiceRequest) -> Result<ServiceResponse, WorkerError> {
            Ok(ServiceResponse {
                data: serde_json::json!({"ok": true}),
                worker: self.name.clone(),
                model: None,
                usage: None,
            })
        }
        async fn health_check(&self) -> Result<HealthStatus, WorkerError> {
            Ok(HealthStatus::Healthy)
        }
        fn api_keys(&self) -> &[String] {
            &[]
        }
        fn rotate_api_key(&mut self) {}
    }

    fn make_request() -> ServiceRequest {
        ServiceRequest {
            action: "chat".into(),
            payload: serde_json::json!({"test": true}),
            params: HashMap::new(),
            user_context: UserContext {
                sub: "test".into(),
                org_id: None,
                scopes: vec![],
                oauth_tokens: HashMap::new(),
            },
        }
    }

    #[tokio::test]
    async fn pipeline_runs_failover() {
        let reg = WorkerRegistry::new();
        reg.register(Box::new(OkWorker { name: "deepseek".into() }));
        let router = Router::new(reg, "llm");
        let result = router.route(make_request()).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().worker, "deepseek");
    }

    #[tokio::test]
    async fn pipeline_block_step_stops_before_failover() {
        let reg = WorkerRegistry::new();
        reg.register(Box::new(OkWorker { name: "deepseek".into() }));
        let router = Router::new(reg, "llm").with_step(Box::new(BlockStep { status: 429 }));
        let result = router.route(make_request()).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            WorkerError::HttpError { status, .. } => assert_eq!(status, 429),
            _ => panic!("expected HttpError"),
        }
    }

    #[tokio::test]
    async fn pipeline_pass_step_continues_to_failover() {
        let reg = WorkerRegistry::new();
        reg.register(Box::new(OkWorker { name: "deepseek".into() }));
        let router = Router::new(reg, "llm").with_step(Box::new(PassStep));
        let result = router.route(make_request()).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().worker, "deepseek");
    }

    #[tokio::test]
    async fn pipeline_no_workers_returns_error() {
        let reg = WorkerRegistry::new();
        let router = Router::new(reg, "llm");
        let result = router.route(make_request()).await;
        assert!(result.is_err());
    }
}
