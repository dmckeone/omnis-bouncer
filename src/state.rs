use reqwest::Client;
use std::ops::Deref;
use std::sync::Arc;
use std::time::Duration;

use crate::queue::{QueueControl, StoreCapacity};
use crate::upstream::UpstreamPool;

#[derive(Debug)]
pub struct Config {
    pub app_name: String,
    pub cookie_secret_key: axum_extra::extract::cookie::Key,
    pub id_cookie_name: String,
    pub position_cookie_name: String,
    pub queue_size_cookie_name: String,
    pub id_http_header: String,
    pub position_http_header: String,
    pub queue_size_http_header: String,
    pub connect_timeout: Duration,
    pub cookie_id_expiration: Duration,
    pub sticky_session_timeout: Duration,
    pub asset_cache_secs: Duration,
    pub http_port: u16,
    pub https_port: u16,
    pub control_port: u16,
    pub queue_enabled: bool,
    pub store_capacity: StoreCapacity,
    pub queue_prefix: String,
    pub quarantine_expiry: Duration,
    pub validated_expiry: Duration,
}

// Our app state type
#[derive(Clone)]
pub struct AppState(Arc<State>);

pub struct State {
    pub config: Config,
    pub queue: QueueControl,
    pub upstream_pool: UpstreamPool,
    pub client: Client,
}

impl AppState {
    pub fn new(
        config: Config,
        queue: QueueControl,
        upstream_pool: UpstreamPool,
        client: Client,
    ) -> Self {
        Self(Arc::new(State {
            config,
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

// this impl tells `PrivateCookieJar` how to access the key from our state
impl axum::extract::FromRef<AppState> for axum_extra::extract::cookie::Key {
    fn from_ref(state: &AppState) -> Self {
        state.config.cookie_secret_key.clone()
    }
}
