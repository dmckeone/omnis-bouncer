use std::sync::Arc;
use tokio::{join, select, sync::Notify, time::sleep};
use tracing::{error, info};

use crate::constants::BACKGROUND_SLEEP_TIME;
use crate::state::AppState;

pub async fn run(state: AppState, shutdown_notifier: Arc<Notify>) {
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
}

/// Tasks that run periodically in the background
async fn background_tasks(state: AppState) {
    let _ = join!(web_tasks(state.clone()), queue_tasks(state.clone()));
}

/// Web
async fn web_tasks(state: AppState) {
    let ids = state.upstream_pool.expire_sticky_sessions().await;
    if !ids.is_empty() {
        info!("Expired {} sticky sessions", ids.len());
    }
}

/// Queue
async fn queue_tasks(state: AppState) {
    // Flush all emit buffer entries
    state.queue.flush_event_throttle_buffer(None).await;

    // Verify waiting page
    let queue_prefix = state.config.queue_prefix.clone();
    for locale in state.config.locales.iter() {
        state.queue.verify_waiting_page(&queue_prefix, locale).await;
    }

    if state.config.queue_rotation_enabled {
        // Queue rotation
        let result = state.queue.rotate_full(&queue_prefix, None).await;

        match result {
            Ok(rotate) => {
                if rotate.has_changes() {
                    info!(
                        "Queue rotation -- queue expired: {}  store expired: {}  promoted: {}",
                        rotate.queue_expired, rotate.store_expired, rotate.promoted
                    )
                }
            }
            Err(e) => error!("Failed to rotate queue: {:?}", e),
        }
    }
}
