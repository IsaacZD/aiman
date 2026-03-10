use axum::{routing::get, Router};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() {
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new("info"))
        .with(tracing_subscriber::fmt::layer())
        .init();

    let app = Router::new().route("/health", get(health));

    let bind_addr = std::env::var("AIMAN_BIND").unwrap_or_else(|_| "0.0.0.0:4010".to_string());
    let listener = tokio::net::TcpListener::bind(&bind_addr)
        .await
        .expect("bind host listener");

    tracing::info!("aiman-host listening on {bind_addr}");
    axum::serve(listener, app).await.expect("serve host");
}

async fn health() -> &'static str {
    "ok"
}
