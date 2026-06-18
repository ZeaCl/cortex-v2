use serde::{Deserialize, Serialize};

#[derive(Debug, thiserror::Error)]
pub enum WorkerError {
    #[error("HTTP {status}: {message}")]
    HttpError { status: u16, message: String },

    #[error("Rate limited: {0}")]
    RateLimited(String),

    #[error("Auth failed: {0}")]
    AuthFailed(String),

    #[error("Timeout after {0}ms")]
    Timeout(u64),

    #[error("Config error: {0}")]
    ConfigError(String),

    #[error("Parse error: {0}")]
    ParseError(String),

    #[error(transparent)]
    Other(anyhow::Error),
}

impl From<anyhow::Error> for WorkerError {
    fn from(e: anyhow::Error) -> Self {
        WorkerError::Other(e)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum HealthStatus {
    Healthy,
    Degraded { reason: String },
    Unavailable { reason: String },
}
