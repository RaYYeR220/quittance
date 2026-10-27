//! Quittance API: verify payment disclosures, run "ask for proof" challenges,
//! and serve the live demo.

mod api;
mod chain;
mod challenges;
mod demo;

use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use tower_http::{cors::CorsLayer, limit::RequestBodyLimitLayer, trace::TraceLayer};
use tracing_subscriber::EnvFilter;
use zip311::Network;

fn env_list(key: &str, default: &str) -> Vec<String> {
    std::env::var(key)
        .unwrap_or_else(|_| default.to_string())
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

#[tokio::main]
async fn main() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    tracing_subscriber::fmt().with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into())).init();

    let bind: SocketAddr = std::env::var("QUITTANCE_BIND").unwrap_or_else(|_| "0.0.0.0:8080".into()).parse().expect("QUITTANCE_BIND");
    let data = std::env::var("QUITTANCE_DATA").map(PathBuf::from).ok();
    let mut endpoints = HashMap::new();
    endpoints.insert(Network::Main, env_list("QUITTANCE_LWD_MAIN", "https://zec.rocks:443,https://eu.zec.rocks:443,https://na.zec.rocks:443"));
    endpoints.insert(Network::Test, env_list("QUITTANCE_LWD_TEST", "https://testnet.zec.rocks:443"));

    let chain = chain::Chain::new(&endpoints).expect("lightwalletd endpoints");
    let gallery = data
        .as_ref()
        .and_then(|d| std::fs::read(d.join("gallery.json")).ok())
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_else(|| serde_json::json!([]));
    let vk = tokio::task::spawn_blocking(zip311::verifying_key).await.expect("verifying key");
    let demo = match (std::env::var("QUITTANCE_DEMO_SENDER"), std::env::var("QUITTANCE_DEMO_PAYEE"), data.as_ref()) {
        (Ok(sender), Ok(payee), Some(data)) => {
            let pk = Arc::new(tokio::task::spawn_blocking(zip311::proving_key).await.expect("proving key"));
            let servers = endpoints.get(&Network::Main).cloned().unwrap_or_default();
            match demo::Demo::load(data, sender.into(), payee.into(), servers, pk) {
                Ok(d) => Some(d),
                Err(e) => {
                    tracing::warn!("live demo disabled: {e}");
                    None
                }
            }
        }
        _ => None,
    };

    let state = Arc::new(api::AppState {
        chain,
        vk,
        tickets: challenges::Store::open(data.as_ref().map(|d| d.join("tickets.json"))),
        demo,
        app_origin: std::env::var("QUITTANCE_APP_ORIGIN").unwrap_or_else(|_| "http://localhost:3000".into()),
        gallery,
        limiter: api::Limiter::new(),
    });

    let app = api::router(state)
        .layer(RequestBodyLimitLayer::new(4 << 20))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http());

    tracing::info!("listening on {bind}");
    let listener = tokio::net::TcpListener::bind(bind).await.expect("bind");
    axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>())
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await
        .expect("server");
}
