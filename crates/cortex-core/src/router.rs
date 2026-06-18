use crate::registry::WorkerRegistry;
use crate::types::ServiceRequest;

/// El Router elige qué worker maneja cada request.
pub struct Router {
    registry: WorkerRegistry,
}

impl Router {
    pub fn new(registry: WorkerRegistry) -> Self {
        Self { registry }
    }

    /// Dado un tipo de servicio, devuelve los nombres de los workers disponibles.
    pub fn candidates(&self, service_type: &str) -> Vec<String> {
        self.registry.list_by_service_type(service_type)
    }

    /// Intenta ejecutar un request en el primer worker disponible.
    pub fn route(&self, service_type: &str, _request: ServiceRequest) -> Result<String, String> {
        let names = self.candidates(service_type);
        if names.is_empty() {
            return Err("no workers".into());
        }
        let worker = self.registry.get(&names[0]).ok_or("no workers")?;
        Ok(worker.name().to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{ServiceRequest, ServiceResponse, UserContext};
    use crate::worker::Worker;
    use crate::worker::error::{HealthStatus, WorkerError};
    use async_trait::async_trait;
    use std::collections::HashMap;

    /// Worker falso para tests.
    struct MockWorker {
        name: String,
        service_type: String,
        priority: u8,
    }

    #[async_trait]
    impl Worker for MockWorker {
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
            self.priority
        }
        async fn execute(&self, _request: ServiceRequest) -> Result<ServiceResponse, WorkerError> {
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

    fn make_request(action: &str) -> ServiceRequest {
        ServiceRequest {
            action: action.to_string(),
            payload: serde_json::json!({"test": true}),
            params: HashMap::new(),
            user_context: UserContext {
                sub: "test-user".to_string(),
                org_id: None,
                scopes: vec![],
                oauth_tokens: HashMap::new(),
            },
        }
    }

    #[test]
    fn candidates_returns_matching_workers() {
        let reg = WorkerRegistry::new();
        reg.register(Box::new(MockWorker {
            name: "deepseek".into(),
            service_type: "llm".into(),
            priority: 10,
        }));
        reg.register(Box::new(MockWorker {
            name: "brave".into(),
            service_type: "search".into(),
            priority: 20,
        }));

        let router = Router::new(reg);

        let llm = router.candidates("llm");
        assert_eq!(llm.len(), 1);
        assert!(llm.contains(&"deepseek".to_string()));

        let search = router.candidates("search");
        assert_eq!(search.len(), 1);
        assert!(search.contains(&"brave".to_string()));

        let empty = router.candidates("tax");
        assert!(empty.is_empty());
    }

    #[test]
    fn route_picks_first_worker() {
        let reg = WorkerRegistry::new();
        reg.register(Box::new(MockWorker {
            name: "deepseek".into(),
            service_type: "llm".into(),
            priority: 10,
        }));

        let router = Router::new(reg);
        let result = router.route("llm", make_request("chat"));
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "deepseek");
    }

    #[test]
    fn route_fails_when_no_workers() {
        let reg = WorkerRegistry::new();
        let router = Router::new(reg);

        let result = router.route("llm", make_request("chat"));
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "no workers");
    }
}
