mod app;
mod background;
mod config;
mod constants;
mod control;
mod cookies;
mod database;
mod errors;
mod omnis;
mod queue;
mod secrets;
mod servers;
mod signals;
mod state;
mod stream;
mod upstream;
mod waiting_room;

use axum_server::Handle;
use config::Config;
use std::sync::Arc;
use tokio::sync::Notify;
use tracing::{info, Level};

use crate::secrets::decode_master_key;

fn main() {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_max_level(Level::INFO)
        .with_target(false)
        .compact()
        .init();

    // Install crypto provider guard (must be early in app startup)
    rustls::crypto::aws_lc_rs::default_provider()
        .install_default()
        .expect("Failed to install aws_lc_rs for crypto provider");

    // Create a shutdown handle for graceful shutdown  (must be early in app startup)
    let shutdown_handle = Handle::new();
    let stream_notify = Arc::new(Notify::new());
    let background_notify = Arc::new(Notify::new());

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("Failed to build tokio runtime");

    let config = config();

    runtime.block_on(app::run(
        config,
        shutdown_handle,
        stream_notify,
        background_notify,
    ));

    info!("Shutdown complete");
}

fn config() -> Config {
    // TODO: Move cookie master key into configuration parsing
    let base64_master_key =
        "Fkm+v0BDS+XoGNTlfsjLoH97DtqsQL4L2KFB8OkWxk/izMiXgfTE1IoY8MxG7ANYuXCFkpUFstD33Rhq/w03vQ==";

    Config {
        cookie_secret_key: decode_master_key(base64_master_key)
            .expect("Failed to decode cookie master key"),
        ..Config::default()
    }
}
