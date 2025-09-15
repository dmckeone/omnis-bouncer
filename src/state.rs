use reqwest::Client;
use std::time::Duration;

use crate::queue::{QueueControl, StoreCapacity};
use crate::upstream::UpstreamPool;

#[derive(Debug)]
pub struct Config {
    pub app_name: String,
    pub cookie_name: String,
    pub header_name: String,
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
pub struct AppState {
    pub config: Config,
    pub queue: QueueControl,
    pub upstream_pool: UpstreamPool,
    pub client: Client,
}
