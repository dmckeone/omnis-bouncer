use std::sync::Arc;
use tokio::sync::Notify;
use tokio::time::sleep;
use tokio::{join, select};
use tracing::{error, info};

use crate::constants::BACKGROUND_SLEEP_TIME;
use crate::errors::Result;
use crate::state::AppState;

pub async fn background_task_loop(state: AppState, shutdown_notifier: Arc<Notify>) -> Result<()> {
    info!("Starting background tasks");
    loop {
        background_tasks(state.clone()).await;

        // Wait for either a shutdown signal or the background sleep time, whichever comes first
        select! {
            _ = shutdown_notifier.notified() => break,
            _ = sleep(BACKGROUND_SLEEP_TIME) => {}
        }
    }
    info!("Shutdown background tasks");
    Ok(())
}

/// Tasks that run periodically in the background
async fn background_tasks(state: AppState) {
    let _ = join!(
        expire_sticky_sessions(state.clone()),
        queue_rotation(state.clone())
    );
}

async fn expire_sticky_sessions(state: AppState) {
    let ids = state.upstream_pool.expire_sticky_sessions().await;
    if !ids.is_empty() {
        info!("Expired {} sticky sessions", ids.len());
    }
}

/// Queue rotation
async fn queue_rotation(state: AppState) {
    // Queue rotation
    let result = state
        .queue
        .rotate_full(state.config.queue_prefix.clone(), None)
        .await;

    match result {
        Ok(rotate) => {
            if rotate.has_changes() {
                info!(
                    "Queue rotation -- queue removed: {}  store removed: {}  promoted: {}",
                    rotate.queue_removed, rotate.store_removed, rotate.promoted
                )
            }
        }
        Err(e) => error!("Failed to rotate queue: {:?}", e),
    }
}
