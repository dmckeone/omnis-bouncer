mod app;
mod background;
mod certs;
mod cli;
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
use std::{path::Path, sync::Arc};
use tokio::sync::Notify;
use tracing::{error, info, Level};

use crate::certs::{write_pem, write_pfx};
use crate::cli::{parse_cli, Commands, ExportAuthorityArgs, ExportAuthorityCommands, RunArgs};
use crate::config::{read_config_file, Config};
use crate::secrets::encode_master_key;

/// Main entry point for app
fn main() {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_max_level(Level::INFO)
        .with_target(false)
        .compact()
        .init();

    // Parse CLI arguments
    let cli = parse_cli();
    match &cli.command {
        Some(Commands::Run(args)) => run_server(args),
        Some(Commands::GenerateKey) => generate_cookie_master_key(),
        Some(Commands::ExportAuthority(args)) => write_certs(args),
        None => {}
    }
}

/// Run the main server
fn run_server(args: &RunArgs) {
    // Build Config
    let config = match Config::try_from(args) {
        Ok(config) => match &args.config_file {
            Some(path) => match read_config_file(path, config) {
                Ok(config) => config,
                Err(err) => {
                    error!("Failed to read configuration file: {}", err);
                    return;
                }
            },
            None => config,
        },
        Err(err) => {
            error!("Failed to read configuration: {:?}", err);
            return;
        }
    };

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

    runtime.block_on(app::run(
        config,
        shutdown_handle,
        stream_notify,
        background_notify,
    ));

    info!("Shutdown complete");
}

/// Generate a cookie master key that can be used to encode cookies
fn generate_cookie_master_key() {
    let key = axum_extra::extract::cookie::Key::generate();
    let encoded = encode_master_key(key);
    println!("\nCookie Key (base64): {}\n", encoded);
}

/// Write out the Certificate Authority public cert for https:// testing
fn write_certs(args: &ExportAuthorityArgs) {
    match &args.command {
        Some(ExportAuthorityCommands::Pfx { path }) => write_pfx(Path::new(path)),
        Some(ExportAuthorityCommands::Pem { path }) => write_pem(Path::new(path)),
        None => {}
    }
}
