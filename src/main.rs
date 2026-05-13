mod auth;
mod config;
mod error;
mod handlers;
mod services;
mod state;

use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use axum::routing::{get, post};
use axum::Router;
use tokio::net::TcpListener;
use tokio::signal;
use tokio::sync::Semaphore;
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::trace::TraceLayer;
use tracing_subscriber::EnvFilter;

use crate::config::Config;
use crate::state::AppState;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .with_target(false)
        .compact()
        .init();

    let config = Config::from_env().context("loading configuration")?;

    tokio::fs::create_dir_all(&config.output_dir)
        .await
        .with_context(|| format!("creating output dir {}", config.output_dir.display()))?;

    let http_client = reqwest::Client::builder()
        .timeout(Duration::from_secs(config.download_timeout_secs))
        .connect_timeout(Duration::from_secs(5))
        .pool_idle_timeout(Duration::from_secs(90))
        .pool_max_idle_per_host(32)
        .user_agent(concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION")))
        .build()
        .context("building reqwest client")?;

    let resize_semaphore = Arc::new(Semaphore::new(config.resize_workers));
    let max_body_bytes = config.max_body_bytes;
    let bind_addr = config.bind_addr.clone();

    let state = Arc::new(AppState {
        config,
        http_client,
        resize_semaphore,
    });

    let protected = Router::new()
        .route("/images/resize", post(handlers::resize::resize))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            auth::require_bearer_token,
        ));

    let app = Router::new()
        .route("/health", get(handlers::health::health))
        .merge(protected)
        .layer(RequestBodyLimitLayer::new(max_body_bytes))
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let listener = TcpListener::bind(&bind_addr)
        .await
        .with_context(|| format!("binding to {bind_addr}"))?;

    tracing::info!(addr = %bind_addr, "image_proccesor listening");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("server error")?;

    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    tracing::info!("shutdown signal received, draining...");
}
