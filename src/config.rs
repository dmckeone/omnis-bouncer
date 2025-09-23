use std::time::Duration;

use crate::queue::StoreCapacity;
use crate::upstream::Upstream;

#[derive(Debug)]
pub struct Config {
    pub app_name: String,
    pub cookie_secret_key: axum_extra::extract::cookie::Key,
    pub redis_uri: String,
    pub initial_upstream: Vec<Upstream>,
    pub public_tls_pair: (Vec<u8>, Vec<u8>),
    pub monitor_tls_pair: (Vec<u8>, Vec<u8>),
    pub id_cookie_name: String,
    pub position_cookie_name: String,
    pub queue_size_cookie_name: String,
    pub position_http_header: String,
    pub queue_size_http_header: String,
    pub acquire_timeout: Duration,
    pub connect_timeout: Duration,
    pub cookie_id_expiration: Duration,
    pub sticky_session_timeout: Duration,
    pub asset_cache_secs: Duration,
    pub buffer_connections: usize,
    pub js_client_rate_limit_per_sec: u64,
    pub api_rate_limit_per_sec: u64,
    pub ultra_rate_limit_per_sec: u64,
    pub http_port: u16,
    pub https_port: u16,
    pub control_port: u16,
    pub queue_enabled: bool,
    pub queue_rotation_enabled: bool,
    pub store_capacity: StoreCapacity,
    pub queue_prefix: String,
    pub quarantine_expiry: Duration,
    pub validated_expiry: Duration,
    pub publish_throttle: Duration,
}
