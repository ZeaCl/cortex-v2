use axum::{Router, routing::get};

#[tokio::main]
async fn main() {
    let contenido = std::fs::read_to_string("cortex.toml").unwrap();
    let config: cortex_core::config::AppConfig = toml::from_str(&contenido).unwrap();

    let app = Router::new().route("/api/health", get(health));

    let addr = format!("{}:{}", config.gateway.host, config.gateway.port);
    println!("Cortex v2 escuchando en http://{addr}");

    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn health() -> &'static str {
    "OK"
}
