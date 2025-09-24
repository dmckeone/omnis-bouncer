use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::config;
use crate::queue::{QueueEvent, QueueSettings, QueueStatus};
use crate::upstream;

#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(
    examples(
        json!({"uri": "http://127.0.0.1:63111", "connections": 100, "sticky_sessions": 10})
    )
)]
pub struct Upstream {
    uri: String,
    connections: usize,
    sticky_sessions: usize,
}

impl From<&upstream::Upstream> for Upstream {
    fn from(upstream: &upstream::Upstream) -> Self {
        Self {
            uri: upstream.uri.clone(),
            connections: upstream.connections,
            sticky_sessions: upstream.sticky_sessions,
        }
    }
}

impl From<&Upstream> for upstream::Upstream {
    fn from(upstream: &Upstream) -> Self {
        Self {
            uri: upstream.uri.clone(),
            connections: upstream.connections,
            sticky_sessions: upstream.sticky_sessions,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(
    examples(
        json!({"uri": "http://127.0.0.1:63111"})
    )
)]
pub struct UpstreamRemove {
    pub(crate) uri: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(
    examples(
        json!({"name":"Omnis Bouncer","redis_uri":"redis://127.0.0.1","config_upstream":[{"uri":"http://127.0.0.1:63111","connections":100,"sticky_sessions":100}],"id_cookie_name":"omnis-bouncer-id","position_cookie_name":"omnis-bouncer-queue-position","queue_size_cookie_name":"omnis-bouncer-queue-size","position_http_header":"x-omnis-bouncer-queue-position","queue_size_http_header":"x-omnis-bouncer-queue-size","acquire_timeout":10,"connect_timeout":10,"cookie_id_expiration":86400,"sticky_session_timeout":600,"asset_cache_secs":60,"buffer_connections":1000,"js_client_rate_limit_per_sec":0,"api_rate_limit_per_sec":10,"ultra_rate_limit_per_sec":10,"public_http_port":3000,"public_https_port":3001,"monitor_https_port":2999,"queue_enabled":true,"queue_rotation_enabled":true,"store_capacity":5,"redis_prefix":"omnis_bouncer","quarantine_expiry":45,"validated_expiry":600,"publish_throttle":0})
    )
)]
pub struct Config {
    pub name: String,
    pub redis_uri: String,
    pub config_upstream: Vec<Upstream>,
    pub id_cookie_name: String,
    pub position_cookie_name: String,
    pub id_upstream_http_header: String,
    pub id_evict_upstream_http_header: String,
    pub queue_size_cookie_name: String,
    pub position_http_header: String,
    pub queue_size_http_header: String,
    pub acquire_timeout: u64,
    pub connect_timeout: u64,
    pub cookie_id_expiration: u64,
    pub sticky_session_timeout: u64,
    pub asset_cache_secs: u64,
    pub buffer_connections: usize,
    pub js_client_rate_limit_per_sec: u64,
    pub api_rate_limit_per_sec: u64,
    pub ultra_rate_limit_per_sec: u64,
    pub public_http_port: u16,
    pub public_https_port: u16,
    pub monitor_https_port: u16,
    pub queue_enabled: bool,
    pub queue_rotation_enabled: bool,
    pub store_capacity: isize,
    pub redis_prefix: String,
    pub quarantine_expiry: u64,
    pub validated_expiry: u64,
    pub publish_throttle: u64,
    pub ultra_thin_inject_headers: bool,
    pub fallback_ultra_thin_library: Option<String>,
    pub fallback_ultra_thin_class: Option<String>,
}

impl From<&config::Config> for Config {
    fn from(config: &config::Config) -> Self {
        Self {
            name: config.app_name.clone(),
            redis_uri: config.redis_uri.clone(),
            config_upstream: config.initial_upstream.iter().map(Upstream::from).collect(),
            id_cookie_name: config.id_cookie_name.clone(),
            position_cookie_name: config.position_cookie_name.clone(),
            queue_size_cookie_name: config.queue_size_cookie_name.clone(),
            id_upstream_http_header: config.id_upstream_http_header.clone(),
            id_evict_upstream_http_header: config.id_evict_upstream_http_header.clone(),
            position_http_header: config.position_http_header.clone(),
            queue_size_http_header: config.queue_size_http_header.clone(),
            acquire_timeout: config.acquire_timeout.as_secs(),
            connect_timeout: config.connect_timeout.as_secs(),
            cookie_id_expiration: config.cookie_id_expiration.as_secs(),
            sticky_session_timeout: config.sticky_session_timeout.as_secs(),
            asset_cache_secs: config.asset_cache_secs.as_secs(),
            buffer_connections: config.buffer_connections,
            js_client_rate_limit_per_sec: config.js_client_rate_limit_per_sec,
            api_rate_limit_per_sec: config.api_rate_limit_per_sec,
            ultra_rate_limit_per_sec: config.ultra_rate_limit_per_sec,
            public_http_port: config.http_port,
            public_https_port: config.https_port,
            monitor_https_port: config.control_port,
            queue_enabled: config.queue_enabled,
            queue_rotation_enabled: config.queue_rotation_enabled,
            store_capacity: isize::from(config.store_capacity),
            redis_prefix: config.queue_prefix.clone(),
            quarantine_expiry: config.quarantine_expiry.as_secs(),
            validated_expiry: config.validated_expiry.as_secs(),
            publish_throttle: config.publish_throttle.as_secs(),
            ultra_thin_inject_headers: config.ultra_thin_inject_headers,
            fallback_ultra_thin_library: config.fallback_ultra_thin_library.clone(),
            fallback_ultra_thin_class: config.fallback_ultra_thin_class.clone(),
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
#[schema(
    examples(
        json!({"queue_enabled": true, "store_capacity": 10, "updated": "2025-09-23T10:44"})
    )
)]
pub struct Settings {
    pub queue_enabled: bool,
    pub store_capacity: isize,
    pub updated: Option<DateTime<Utc>>,
}

impl From<QueueSettings> for Settings {
    fn from(settings: QueueSettings) -> Self {
        Self {
            queue_enabled: settings.enabled,
            store_capacity: settings.capacity.into(),
            updated: settings.updated,
        }
    }
}

#[derive(Debug, Deserialize, ToSchema)]
#[schema(
    examples(
        json!({"queue_enabled": true, "store_capacity": 10})
    )
)]
pub struct SettingsPatch {
    #[serde(default)]
    pub queue_enabled: Option<bool>,
    #[serde(default)]
    pub store_capacity: Option<isize>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(
    examples(
        json!({"queue_enabled": true, "store_capacity": 10, "queue_size": 100, "store_size": 10, "updated": "2025-09-23T10:44"})
    )
)]
pub struct Status {
    pub queue_enabled: bool,
    pub store_capacity: isize,
    pub queue_size: usize,
    pub store_size: usize,
    pub updated: Option<DateTime<Utc>>,
}

impl From<QueueStatus> for Status {
    fn from(status: QueueStatus) -> Self {
        Self {
            queue_enabled: status.enabled,
            store_capacity: status.capacity.into(),
            queue_size: status.queue_size,
            store_size: status.store_size,
            updated: status.updated,
        }
    }
}

#[derive(Debug, Clone)]
pub enum Event {
    SettingsChanged,
    WaitingPageChanged,
    QueueAdded,
    QueueExpired,
    QueueRemoved,
    StoreAdded,
    StoreExpired,
}

impl From<QueueEvent> for Event {
    fn from(queue_event: QueueEvent) -> Self {
        match queue_event {
            QueueEvent::SettingsChanged => Self::SettingsChanged,
            QueueEvent::WaitingPageChanged => Self::WaitingPageChanged,
            QueueEvent::QueueAdded => Self::QueueAdded,
            QueueEvent::QueueExpired => Self::QueueExpired,
            QueueEvent::StoreAdded => Self::StoreAdded,
            QueueEvent::StoreExpired => Self::StoreExpired,
            QueueEvent::QueueRemoved => Self::QueueRemoved,
        }
    }
}

impl From<Event> for String {
    fn from(event: Event) -> Self {
        match event {
            Event::SettingsChanged => String::from("settings:updated"),
            Event::WaitingPageChanged => String::from("waiting_page:updated"),
            Event::QueueAdded => String::from("queue:added"),
            Event::QueueExpired => String::from("queue:expired"),
            Event::StoreAdded => String::from("store:added"),
            Event::StoreExpired => String::from("store:expired"),
            Event::QueueRemoved => String::from("queue:removed"),
        }
    }
}
