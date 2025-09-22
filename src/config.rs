use std::time::Duration;

use crate::constants::{SELF_SIGNED_CERT, SELF_SIGNED_KEY};
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
    pub store_capacity: StoreCapacity,
    pub queue_prefix: String,
    pub quarantine_expiry: Duration,
    pub validated_expiry: Duration,
    pub emit_throttle: Duration,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            app_name: String::from("Omnis Studio Bouncer"),
            cookie_secret_key: axum_extra::extract::cookie::Key::generate(),
            redis_uri: String::from("redis://127.0.0.1/"),
            initial_upstream: vec![Upstream::new("http://127.0.0.1:63111", 100, 10)],
            public_tls_pair: (SELF_SIGNED_CERT.to_vec(), SELF_SIGNED_KEY.to_vec()),
            monitor_tls_pair: (SELF_SIGNED_CERT.to_vec(), SELF_SIGNED_KEY.to_vec()),
            id_cookie_name: String::from("omnis-bouncer-id"),
            position_cookie_name: String::from("omnis-bouncer-queue-position"),
            queue_size_cookie_name: String::from("omnis-bouncer-queue-size"),
            position_http_header: String::from("x-omnis-bouncer-queue-position").to_lowercase(), // Must be lowercase
            queue_size_http_header: String::from("x-omnis-bouncer-queue-size").to_lowercase(), // Must be lowercase
            acquire_timeout: Duration::from_secs(10),
            connect_timeout: Duration::from_secs(10),
            cookie_id_expiration: Duration::from_secs(60 * 60 * 24), // 1 day
            sticky_session_timeout: Duration::from_secs(60 * 10),    // 10 minutes
            asset_cache_secs: Duration::from_secs(60),
            buffer_connections: 10000,
            js_client_rate_limit_per_sec: 0,
            api_rate_limit_per_sec: 0,
            ultra_rate_limit_per_sec: 0,
            http_port: 3000,
            https_port: 3001,
            control_port: 2999,
            queue_enabled: true,
            store_capacity: StoreCapacity::Sized(5),
            queue_prefix: String::from("omnis_bouncer"),
            quarantine_expiry: Duration::from_secs(45),
            validated_expiry: Duration::from_secs(600),
            emit_throttle: Duration::from_millis(100),
        }
    }
}
