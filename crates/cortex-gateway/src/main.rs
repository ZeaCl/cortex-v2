fn main() {
    let contenido = std::fs::read_to_string("cortex.toml").unwrap();
    let config: cortex_core::config::AppConfig = toml::from_str(&contenido).unwrap();
    println!("{:#?}", config);
}
