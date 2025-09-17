use axum_server::Handle;
use futures_util::{pin_mut, Stream, StreamExt};
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::sync::Notify;
use tokio::time::sleep;
use tokio::{select, signal};
use tracing::info;

use crate::constants::SHUTDOWN_TIMEOUT;

/// Ensure that a stream can be cancelled by a notifier
pub fn cancellable<S>(stream: S, cancel: Arc<Notify>) -> impl Stream<Item = S::Item>
where
    S: Stream,
{
    async_stream::stream! {
        pin_mut!(stream);

        loop {
            select! {
                Some(item) = stream.next() => {
                    yield item
                }
                _ = cancel.notified() => {
                    break;
                }
            }
        }
    }
}

/// Future for monitoring a shutdown signal to gracefully shut down the server
pub async fn shutdown_signal(
    handle: Handle,
    stream_notify: Arc<Notify>,
    background_notify: Arc<Notify>,
) -> anyhow::Result<()> {
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

    // Wait for Ctrl-C or Terminate, whichever comes first
    select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    info!("Received shutdown signal");

    // Notify all axum_server instances to begin graceful shutdown
    info!("Shutdown web server");
    let shutdown_duration = SHUTDOWN_TIMEOUT;
    handle.graceful_shutdown(Some(shutdown_duration));

    // Notify any streams that they need to start shutting down
    stream_notify.notify_waiters();

    // Show connection count in second increments as the server shuts down
    let start = SystemTime::now();
    while handle.connection_count() > 0
        || SystemTime::now().duration_since(start)? > shutdown_duration
    {
        info!("Connections Remaining: {}", handle.connection_count());
        sleep(Duration::from_secs(1)).await;
    }

    // Notify any tasks waiting for shutdown after web server has completed
    background_notify.notify_waiters();

    Ok(())
}
