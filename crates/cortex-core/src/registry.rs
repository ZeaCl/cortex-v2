
use crate::worker::Worker;
use dashmap::DashMap;

pub struct WorkerRegistry {
    workers: DashMap<String, Box<dyn Worker>>,
}

impl WorkerRegistry {
    pub fn new() -> Self {
        Self { workers: DashMap::new() }
    }

    pub fn register(&self, worker: Box<dyn Worker>) {
        self.workers.insert(worker.name().to_string(), worker);
    }

    /// Devuelve los nombres de los workers de un tipo de servicio.                                                                 
    pub fn list_by_service_type(&self, service_type: &str) -> Vec<String> {
        self.workers
            .iter()
            .filter(|entry| entry.value().service_type() == service_type)
            .map(|entry| entry.key().clone())
            .collect()
    }

    /// Obtiene un worker por nombre. Devuelve una referencia protegida por el lock de DashMap.                                     
    pub fn get(
        &self,
        name: &str,
    ) -> Option<dashmap::mapref::one::Ref<'_, String, Box<dyn Worker>>> {
        self.workers.get(name)
    }
}
