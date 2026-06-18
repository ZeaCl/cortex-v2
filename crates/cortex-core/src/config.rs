use serde::Deserialize;

/// Configuración completa del gateway, leída desde cortex.toml
#[derive(Debug, Clone, Deserialize)]
pub struct AppConfig {
    pub gateway: GatewayConfig,

    #[serde(default)]
    pub thalamus: Option<ThalamusConfig>,

    #[serde(default)]
    pub router: Option<RouterConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GatewayConfig {
    pub host: String,
    pub port: u16,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ThalamusConfig {
    pub jwks_url: String,

    #[serde(default = "default_jwks_cache_ttl")]
    pub jwks_cache_ttl_secs: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RouterConfig {
    #[serde(default = "default_max_concurrent")]
    pub max_concurrent_requests: usize,

    #[serde(default = "default_per_worker")]
    pub max_concurrent_per_worker: usize,
}

fn default_jwks_cache_ttl() -> u64 {
    3600
}
fn default_max_concurrent() -> usize {
    100
}
fn default_per_worker() -> usize {
    20
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_minimal_config() {
        let toml = r#"
[gateway]
host = "0.0.0.0"
port = 4000
"#;
        let cfg: AppConfig = toml::from_str(toml).unwrap();
        assert_eq!(cfg.gateway.host, "0.0.0.0");
        assert_eq!(cfg.gateway.port, 4000);
        assert!(cfg.thalamus.is_none());
        assert!(cfg.router.is_none());
    }

    #[test]
    fn defaults_are_applied_when_fields_missing() {
        let toml = r#"
[gateway]
host = "127.0.0.1"
port = 8080

[router]
max_concurrent_requests = 50
"#;
        let cfg: AppConfig = toml::from_str(toml).unwrap();
        let router = cfg.router.unwrap();
        assert_eq!(router.max_concurrent_requests, 50);
        assert_eq!(router.max_concurrent_per_worker, 20); // default
    }

    #[test]
    fn all_fields_can_be_set() {
        let toml = r#"
[gateway]
host = "0.0.0.0"
port = 3000

[thalamus]
jwks_url = "https://auth.zea.cl/.well-known/jwks.json"
jwks_cache_ttl_secs = 7200

[router]
max_concurrent_requests = 200
max_concurrent_per_worker = 50
"#;
        let cfg: AppConfig = toml::from_str(toml).unwrap();
        let t = cfg.thalamus.unwrap();
        assert_eq!(t.jwks_cache_ttl_secs, 7200);

        let r = cfg.router.unwrap();
        assert_eq!(r.max_concurrent_requests, 200);
    }
}
