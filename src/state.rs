use reqwest::Client;
use std::{ops::Deref, sync::Arc};
use tokio::sync::Notify;

use crate::config::Config;
use crate::queue::QueueControl;
use crate::upstream::UpstreamPool;

// Our app state type
#[derive(Clone)]
pub struct AppState(Arc<State>);

pub struct State {
    pub config: Config,
    pub shutdown_notifier: Arc<Notify>,
    pub queue: QueueControl,
    pub upstream_pool: UpstreamPool,
    pub client: Client,
}

impl AppState {
    pub fn new(
        config: Config,
        shutdown_notifier: Arc<Notify>,
        queue: QueueControl,
        upstream_pool: UpstreamPool,
        client: Client,
    ) -> Self {
        Self(Arc::new(State {
            config,
            shutdown_notifier,
            queue,
            upstream_pool,
            client,
        }))
    }
}

// deref so you can still access the inner fields easily
impl Deref for AppState {
    type Target = State;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
