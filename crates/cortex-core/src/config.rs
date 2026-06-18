use serde::Deserialize;

/// Configuración completa del gateway, leída desde cortex.toml                                                                     
#[derive(Debug, Clone, Deserialize)]
pub struct AppConfig {
    pub gateway: GatewayConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GatewayConfig {
    pub host: String,
    pub port: u16,
}
