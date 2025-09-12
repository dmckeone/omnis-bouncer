use axum_server::Handle;
use std::time::{Duration, SystemTime};
use tokio::signal;

use crate::constants::SHUTDOWN_TIMEOUT;

/// Future for monitoring a shutdown signal to gracefully shut down the server
pub async fn shutdown_signal(handle: Handle) -> anyhow::Result<()> {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    tracing::info!("Received termination signal shutting down");
    let shutdown_duration = SHUTDOWN_TIMEOUT;
    handle.graceful_shutdown(Some(shutdown_duration));

    // Show connection count in second increments as the server shuts down
    let start = SystemTime::now();
    while handle.connection_count() > 0
        || SystemTime::now().duration_since(start)? > shutdown_duration
    {
        tracing::info!("Alive connections: {}", handle.connection_count());
        tokio::time::sleep(Duration::from_secs(1)).await;
    }

    Ok(())
}
