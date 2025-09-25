use base64::DecodeError;
use core::result::Result;
use resolve_path::PathResolveExt;
use serde::{Deserialize, Serialize};
use std::{
    fmt::Display,
    fs::File,
    io::{self, Read},
    time::Duration,
};
use toml::de;

use crate::constants::{SELF_SIGNED_CERT, SELF_SIGNED_KEY};
use crate::errors::Error;
use crate::queue::StoreCapacity;
use crate::secrets::decode_master_key;
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
    pub id_upstream_http_header: String,
    pub id_evict_upstream_http_header: String,
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
    pub ultra_thin_inject_headers: bool,
    pub fallback_ultra_thin_library: Option<String>,
    pub fallback_ultra_thin_class: Option<String>,
}

impl Config {
    pub fn fallback_enabled(&self) -> bool {
        self.fallback_ultra_thin_library.is_some() && self.fallback_ultra_thin_class.is_some()
    }
}

// Read a single file from a string path
fn read_file(path: impl Into<String>) -> Result<Vec<u8>, io::Error> {
    let path = path.into();
    let path = path.resolve();
    let mut contents: Vec<u8> = Vec::new();
    File::open(path)?.read_to_end(&mut contents)?;
    Ok(contents)
}

// Create a TLS pair given
pub fn build_tls_pair(
    cert_path: Option<String>,
    key_path: Option<String>,
    cert: Option<String>,
    key: Option<String>,
) -> Result<(Vec<u8>, Vec<u8>), io::Error> {
    let pair = match (cert_path, key_path, cert, key) {
        (Some(cert_path), Some(key_path), _, _) => (read_file(cert_path)?, read_file(key_path)?),
        (_, _, Some(cert), Some(key)) => (cert.as_bytes().to_vec(), key.as_bytes().to_vec()),
        _ => (SELF_SIGNED_CERT.to_vec(), SELF_SIGNED_KEY.to_vec()),
    };
    Ok(pair)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConfigFileUpstream {
    pub uri: String,
    pub connections: Option<usize>,
    pub sticky_sessions: Option<usize>,
}

impl From<&ConfigFileUpstream> for Upstream {
    fn from(config: &ConfigFileUpstream) -> Self {
        let defaults = Upstream::default();
        Self {
            uri: config.uri.clone(),
            connections: config.connections.unwrap_or(defaults.connections),
            sticky_sessions: config.sticky_sessions.unwrap_or(defaults.sticky_sessions),
        }
    }
}

pub enum ConfigFileError {
    IOError(io::Error),
    ContentsUnreadable(de::Error),
    InvalidCookieKey(DecodeError),
    StoreCapacityOutOfRange(isize),
    TLSCertificateError(io::Error),
}

impl Display for ConfigFileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigFileError::IOError(e) => write!(f, "{}", e),
            ConfigFileError::ContentsUnreadable(e) => write!(f, "{}", e),
            ConfigFileError::InvalidCookieKey(e) => {
                write!(f, "Cookie Key could not be decoded: {}", e)
            }
            ConfigFileError::StoreCapacityOutOfRange(e) => write!(
                f,
                "Store capacity should be -1 (infinite) or greater than 0: {}",
                e
            ),
            ConfigFileError::TLSCertificateError(e) => {
                write!(f, "Unable to read TLS Certificate: {}", e)
            }
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct ConfigFile {
    pub name: Option<String>,
    pub cookie_secret_key: Option<String>,
    pub redis_uri: Option<String>,
    pub initial_upstream: Option<Vec<ConfigFileUpstream>>,
    pub public_tls_key_path: Option<String>,
    pub public_tls_certificate_path: Option<String>,
    pub monitor_tls_key_path: Option<String>,
    pub monitor_tls_certificate_path: Option<String>,
    pub id_cookie_name: Option<String>,
    pub position_cookie_name: Option<String>,
    pub queue_size_cookie_name: Option<String>,
    pub id_upstream_http_header: Option<String>,
    pub id_evict_upstream_http_header: Option<String>,
    pub position_http_header: Option<String>,
    pub queue_size_http_header: Option<String>,
    pub acquire_timeout: Option<u64>,
    pub connect_timeout: Option<u64>,
    pub cookie_id_expiration: Option<u64>,
    pub sticky_session_timeout: Option<u64>,
    pub asset_cache_secs: Option<u64>,
    pub buffer_connections: Option<usize>,
    pub js_client_rate_limit_per_sec: Option<u64>,
    pub api_rate_limit_per_sec: Option<u64>,
    pub ultra_rate_limit_per_sec: Option<u64>,
    pub public_http_port: Option<u16>,
    pub public_https_port: Option<u16>,
    pub monitor_https_port: Option<u16>,
    pub queue_enabled: Option<bool>,
    pub queue_rotation_enabled: Option<bool>,
    pub store_capacity: Option<isize>,
    pub redis_prefix: Option<String>,
    pub quarantine_expiry: Option<u64>,
    pub validated_expiry: Option<u64>,
    pub publish_throttle: Option<u64>,
    pub ultra_thin_inject_headers: Option<bool>,
    pub fallback_ultra_thin_library: Option<String>,
    pub fallback_ultra_thin_class: Option<String>,
}

/// Read all values set by the configuration file and merge in defaults values, sourced from the CLI
fn merge_config(config: Config, config_file: ConfigFile) -> Result<Config, ConfigFileError> {
    let has_public_tls = config_file.public_tls_certificate_path.is_some()
        || config_file.public_tls_key_path.is_some();

    let public_tls_pair = if has_public_tls {
        match build_tls_pair(
            config_file.public_tls_certificate_path,
            config_file.public_tls_key_path,
            None,
            None,
        ) {
            Ok(pair) => pair,
            Err(error) => return Err(ConfigFileError::TLSCertificateError(error)),
        }
    } else {
        config.public_tls_pair
    };

    let has_monitor_tls = config_file.monitor_tls_certificate_path.is_some()
        || config_file.monitor_tls_key_path.is_some();

    let monitor_tls_pair = if has_monitor_tls {
        match build_tls_pair(
            config_file.monitor_tls_certificate_path,
            config_file.monitor_tls_key_path,
            None,
            None,
        ) {
            Ok(pair) => pair,
            Err(error) => return Err(ConfigFileError::TLSCertificateError(error)),
        }
    } else {
        config.monitor_tls_pair
    };

    Ok(Config {
        app_name: config_file.name.unwrap_or(config.app_name),
        cookie_secret_key: match config_file.cookie_secret_key {
            Some(key) => match decode_master_key(key) {
                Ok(key) => key,
                Err(error) => return Err(ConfigFileError::InvalidCookieKey(error)),
            },
            None => config.cookie_secret_key,
        },
        redis_uri: config_file.redis_uri.unwrap_or(config.redis_uri),
        initial_upstream: match &config_file.initial_upstream {
            Some(u) => u.iter().map(Upstream::from).collect(),
            None => config.initial_upstream,
        },
        public_tls_pair,
        monitor_tls_pair,
        id_cookie_name: config_file.id_cookie_name.unwrap_or(config.id_cookie_name),
        position_cookie_name: config_file
            .position_cookie_name
            .unwrap_or(config.position_cookie_name),
        queue_size_cookie_name: config_file
            .queue_size_cookie_name
            .unwrap_or(config.queue_size_cookie_name),
        id_upstream_http_header: config_file
            .id_upstream_http_header
            .unwrap_or(config.id_upstream_http_header),
        id_evict_upstream_http_header: config_file
            .id_evict_upstream_http_header
            .unwrap_or(config.id_evict_upstream_http_header),
        position_http_header: config_file
            .position_http_header
            .unwrap_or(config.position_http_header),
        queue_size_http_header: config_file
            .queue_size_http_header
            .unwrap_or(config.queue_size_http_header),
        acquire_timeout: match config_file.acquire_timeout {
            Some(secs) => Duration::from_secs(secs),
            None => config.acquire_timeout,
        },
        connect_timeout: match config_file.connect_timeout {
            Some(secs) => Duration::from_secs(secs),
            None => config.connect_timeout,
        },
        cookie_id_expiration: match config_file.cookie_id_expiration {
            Some(secs) => Duration::from_secs(secs),
            None => config.cookie_id_expiration,
        },
        sticky_session_timeout: match config_file.sticky_session_timeout {
            Some(secs) => Duration::from_secs(secs),
            None => config.sticky_session_timeout,
        },
        asset_cache_secs: match config_file.asset_cache_secs {
            Some(secs) => Duration::from_secs(secs),
            None => config.asset_cache_secs,
        },
        buffer_connections: config_file
            .buffer_connections
            .unwrap_or(config.buffer_connections),
        js_client_rate_limit_per_sec: config_file
            .js_client_rate_limit_per_sec
            .unwrap_or(config.js_client_rate_limit_per_sec),
        api_rate_limit_per_sec: config_file
            .api_rate_limit_per_sec
            .unwrap_or(config.api_rate_limit_per_sec),
        ultra_rate_limit_per_sec: config_file
            .ultra_rate_limit_per_sec
            .unwrap_or(config.ultra_rate_limit_per_sec),
        http_port: config_file.public_http_port.unwrap_or(config.http_port),
        https_port: config_file.public_https_port.unwrap_or(config.https_port),
        control_port: config_file
            .monitor_https_port
            .unwrap_or(config.control_port),
        queue_enabled: config_file.queue_enabled.unwrap_or(config.queue_enabled),
        queue_rotation_enabled: config_file
            .queue_rotation_enabled
            .unwrap_or(config.queue_rotation_enabled),
        store_capacity: match config_file.store_capacity {
            Some(c) => match StoreCapacity::try_from(c) {
                Ok(capacity) => capacity,
                Err(Error::StoreCapacityOutOfRange(_)) => {
                    return Err(ConfigFileError::StoreCapacityOutOfRange(c));
                }
                Err(_) => unreachable!(),
            },
            None => config.store_capacity,
        },
        queue_prefix: config_file.redis_prefix.unwrap_or(config.queue_prefix),
        quarantine_expiry: match config_file.quarantine_expiry {
            Some(secs) => Duration::from_secs(secs),
            None => config.quarantine_expiry,
        },
        validated_expiry: match config_file.validated_expiry {
            Some(secs) => Duration::from_secs(secs),
            None => config.validated_expiry,
        },
        publish_throttle: match config_file.publish_throttle {
            Some(secs) => Duration::from_secs(secs),
            None => config.publish_throttle,
        },
        ultra_thin_inject_headers: config_file
            .ultra_thin_inject_headers
            .unwrap_or(config.ultra_thin_inject_headers),
        fallback_ultra_thin_library: match config_file.fallback_ultra_thin_library {
            Some(library) => Some(library),
            None => config.fallback_ultra_thin_library,
        },
        fallback_ultra_thin_class: match config_file.fallback_ultra_thin_class {
            Some(library) => Some(library),
            None => config.fallback_ultra_thin_class,
        },
    })
}

pub fn read_config_file(
    path: impl Into<String>,
    defaults: Config,
) -> Result<Config, ConfigFileError> {
    let bytes = match read_file(path) {
        Ok(bytes) => bytes,
        Err(error) => return Err(ConfigFileError::IOError(error)),
    };

    let config_file: ConfigFile = match toml::from_slice(&bytes) {
        Ok(v) => v,
        Err(error) => return Err(ConfigFileError::ContentsUnreadable(error)),
    };

    merge_config(defaults, config_file)
}
