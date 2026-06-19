use crate::registry::WorkerRegistry;
use crate::types::{ServiceRequest, ServiceResponse};
use crate::worker::error::WorkerError;

/// El Router elige qué worker maneja cada request, con failover automático.
pub struct Router {
    registry: WorkerRegistry,
}

impl Router {
    pub fn new(registry: WorkerRegistry) -> Self {
        Self { registry }
    }

    pub fn candidates(&self, service_type: &str) -> Vec<String> {
        self.registry.list_by_service_type(service_type)
    }

    /// Ejecuta un request con failover: prueba cada worker en orden.
    /// Si uno falla, intenta con el siguiente.
    /// Si todos fallan, devuelve el último error.
    pub async fn route(
        &self,
        service_type: &str,
        request: ServiceRequest,
    ) -> Result<ServiceResponse, WorkerError> {
        let names = self.candidates(service_type);
        if names.is_empty() {
            return Err(WorkerError::ConfigError("no workers available".into()));
        }

        let mut last_error = WorkerError::ConfigError("unreachable".into());

        for name in &names {
            let worker = match self.registry.get(name) {
                Some(w) => w,
                None => continue,
            };

            match worker.execute(request.clone()).await {
                Ok(response) => return Ok(response),
                Err(e) => {
                    last_error = e;
                    continue;
                }
            }
        }

        Err(last_error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{ServiceRequest, UserContext};
    use crate::worker::Worker;
    use crate::worker::error::HealthStatus;
    use async_trait::async_trait;
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicU32, Ordering};

    /// Worker que siempre responde OK.
    struct OkWorker {
        name: String,
        service_type: String,
    }

    #[async_trait]
    impl Worker for OkWorker {
        fn name(&self) -> &str {
            &self.name
        }
        fn service_type(&self) -> &str {
            &self.service_type
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

    /// Worker que siempre falla (simula API caída).
    struct FailWorker {
        name: String,
        service_type: String,
    }

    #[async_trait]
    impl Worker for FailWorker {
        fn name(&self) -> &str {
            &self.name
        }
        fn service_type(&self) -> &str {
            &self.service_type
        }
        fn capabilities(&self) -> Vec<&str> {
            vec!["chat"]
        }
        fn priority(&self) -> u8 {
            10
        }
        async fn execute(&self, _: ServiceRequest) -> Result<ServiceResponse, WorkerError> {
            Err(WorkerError::HttpError { status: 503, message: "down".into() })
        }
        async fn health_check(&self) -> Result<HealthStatus, WorkerError> {
            Ok(HealthStatus::Degraded { reason: "test".into() })
        }
        fn api_keys(&self) -> &[String] {
            &[]
        }
        fn rotate_api_key(&mut self) {}
    }

    /// Worker que falla las primeras N veces, luego OK.
    struct FlakyWorker {
        name: String,
        service_type: String,
        fail_count: AtomicU32,
        max_fails: u32,
    }

    #[async_trait]
    impl Worker for FlakyWorker {
        fn name(&self) -> &str {
            &self.name
        }
        fn service_type(&self) -> &str {
            &self.service_type
        }
        fn capabilities(&self) -> Vec<&str> {
            vec!["chat"]
        }
        fn priority(&self) -> u8 {
            10
        }
        async fn execute(&self, _: ServiceRequest) -> Result<ServiceResponse, WorkerError> {
            let fails = self.fail_count.fetch_add(1, Ordering::SeqCst);
            if fails < self.max_fails {
                Err(WorkerError::RateLimited("test".into()))
            } else {
                Ok(ServiceResponse {
                    data: serde_json::json!({"ok": true}),
                    worker: self.name.clone(),
                    model: None,
                    usage: None,
                })
            }
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
    async fn route_single_worker_succeeds() {
        let reg = WorkerRegistry::new();
        reg.register(Box::new(OkWorker { name: "deepseek".into(), service_type: "llm".into() }));

        let router = Router::new(reg);
        let result = router.route("llm", make_request()).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().worker, "deepseek");
    }

    #[tokio::test]
    async fn route_fails_when_no_workers() {
        let reg = WorkerRegistry::new();
        let router = Router::new(reg);
        let result = router.route("llm", make_request()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn failover_skips_failed_worker() {
        let reg = WorkerRegistry::new();
        // deepseek falla, openai responde OK
        reg.register(Box::new(FailWorker { name: "deepseek".into(), service_type: "llm".into() }));
        reg.register(Box::new(OkWorker { name: "openai".into(), service_type: "llm".into() }));

        let router = Router::new(reg);
        let result = router.route("llm", make_request()).await;
        assert!(result.is_ok());
        // Debería haber hecho failover a openai
        assert_eq!(result.unwrap().worker, "openai");
    }

    #[tokio::test]
    async fn failover_all_fail_returns_error() {
        let reg = WorkerRegistry::new();
        reg.register(Box::new(FailWorker { name: "deepseek".into(), service_type: "llm".into() }));
        reg.register(Box::new(FailWorker { name: "openai".into(), service_type: "llm".into() }));

        let router = Router::new(reg);
        let result = router.route("llm", make_request()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn flaky_worker_retries_and_succeeds() {
        let reg = WorkerRegistry::new();
        // deepseek falla las primeras 2 veces, la 3ra OK
        reg.register(Box::new(FlakyWorker {
            name: "deepseek".into(),
            service_type: "llm".into(),
            fail_count: AtomicU32::new(0),
            max_fails: 2,
        }));

        let router = Router::new(reg);

        // 1er intento → falla
        let r1 = router.route("llm", make_request()).await;
        assert!(r1.is_err());

        // 2do intento → falla
        let r2 = router.route("llm", make_request()).await;
        assert!(r2.is_err());

        // 3er intento → éxito
        let r3 = router.route("llm", make_request()).await;
        assert!(r3.is_ok());
        assert_eq!(r3.unwrap().worker, "deepseek");
    }
}
