use clap::{ArgAction, Args, Parser, Subcommand};
use std::collections::HashSet;
use std::time::Duration;

use crate::config::{build_tls_pair, Config};
use crate::errors::{Error, Result};
use crate::queue::StoreCapacity;
use crate::secrets::decode_master_key;
use crate::upstream::Upstream;

#[derive(Parser)]
#[command(version, about, long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[allow(clippy::large_enum_variant)]
#[derive(Subcommand)]
pub enum Commands {
    /// Run the server
    Run(RunArgs),
    /// Generate a random cookie key in Base64 format
    GenerateKey,
    /// Emit bundled self-signed CA certificate for testing with HTTPS
    ExportAuthority(ExportAuthorityArgs),
}

#[derive(Args)]
pub struct RunArgs {
    /// Path to the configuration file (if a config file is used, no other commands are allowed)
    #[arg(short = 'c', long = "config", env = "OMNIS_BOUNCER_CONFIG_FILE")]
    pub config_file: Option<String>,

    /// Name of the app in all UI
    #[arg(
        long,
        conflicts_with = "config_file",
        default_value = "Omnis Bouncer",
        env = "OMNIS_BOUNCER_NAME"
    )]
    pub name: String,

    /// Default locale (language) to use when one can not be selected
    #[arg(
        long,
        conflicts_with = "config_file",
        default_value = "en",
        env = "OMNIS_BOUNCER_DEFAULT_LOCALE"
    )]
    pub default_locale: String,

    /// Locales (Languages) supported for the waiting room page
    #[arg(
        long,
        conflicts_with = "config_file",
        num_args = 0..,
        value_delimiter = ',',
        default_value = "en",
        env = "OMNIS_BOUNCER_LOCALES"
    )]
    pub locales: Vec<String>,

    /// Master key (in base64) for cookie encryption
    #[arg(long, conflicts_with = "config_file", env = "OMNIS_BOUNCER_COOKIE_KEY")]
    pub cookie_key: Option<String>,

    /// URI for connecting to Redis
    #[arg(
        long,
        conflicts_with = "config_file",
        default_value = "redis://127.0.0.1",
        env = "OMNIS_BOUNCER_REDIS_URI"
    )]
    pub redis_uri: String,

    /// Initial upstream servers, comma-delimited
    #[arg(
        long,
        conflicts_with = "config_file",
        num_args = 0..,
        value_delimiter = ',',
        env = "OMNIS_BOUNCER_UPSTREAM_URIS"
    )]
    pub upstream: Vec<String>,

    /// Number of connections to use with initial upstream servers (shared between all servers)
    #[arg(
        long,
        conflicts_with = "config_file",
        default_value = "100",
        env = "OMNIS_BOUNCER_UPSTREAM_CONNECTIONS"
    )]
    pub upstream_connections: usize,

    /// Number of sticky session to use with initial upstream servers (shared between all servers)
    #[arg(
        long,
        conflicts_with = "config_file",
        default_value = "10",
        env = "OMNIS_BOUNCER_UPSTREAM_SESSIONS"
    )]
    pub upstream_sessions: usize,

    /// TLS Private Key to use for the publicly accessible server
    #[arg(
        long,
        conflicts_with = "config_file",
        conflicts_with = "public_tls_key_path",
        requires = "public_tls_certificate",
        env = "OMNIS_BOUNCER_PUBLIC_TLS_KEY"
    )]
    pub public_tls_key: Option<String>,

    /// TLS Public Certificate to use for the publicly accessible server
    #[arg(
        long,
        conflicts_with = "config_file",
        conflicts_with = "public_tls_certificate_path",
        requires = "public_tls_key",
        env = "OMNIS_BOUNCER_PUBLIC_TLS_CERTIFICATE"
    )]
    pub public_tls_certificate: Option<String>,

    /// Path to the TLS Private Key to use for the publicly accessible server
    #[arg(
        long,
        conflicts_with = "config_file",
        conflicts_with = "public_tls_key",
        requires = "public_tls_certificate_path",
        env = "OMNIS_BOUNCER_PUBLIC_TLS_KEY_PATH"
    )]
    pub public_tls_key_path: Option<String>,

    /// Path to the TLS Public Certificate to use for the publicly accessible server
    #[arg(
        long,
        conflicts_with = "config_file",
        conflicts_with = "public_tls_certificate",
        requires = "public_tls_key_path",
        env = "OMNIS_BOUNCER_PUBLIC_TLS_CERTIFICATE_PATH"
    )]
    pub public_tls_certificate_path: Option<String>,

    /// TLS Private Key to use for the monitor and control server
    #[arg(
        long,
        conflicts_with = "config_file",
        conflicts_with = "monitor_tls_key_path",
        requires = "monitor_tls_certificate",
        env = "OMNIS_BOUNCER_MONITOR_TLS_KEY"
    )]
    pub monitor_tls_key: Option<String>,

    /// TLS Public Certificate to use for the monitor and control server
    #[arg(
        long,
        conflicts_with = "config_file",
        conflicts_with = "monitor_tls_certificate_path",
        requires = "monitor_tls_key",
        env = "OMNIS_BOUNCER_MONITOR_TLS_CERTIFICATE"
    )]
    pub monitor_tls_certificate: Option<String>,

    /// Path to the TLS Private Key to use for the monitor and control server
    #[arg(
        long,
        conflicts_with = "config_file",
        conflicts_with = "monitor_tls_key",
        requires = "monitor_tls_certificate_path",
        env = "OMNIS_BOUNCER_MONITOR_TLS_KEY"
    )]
    pub monitor_tls_key_path: Option<String>,

    /// Path to the TLS Public Certificate to use for the monitor and control server
    #[arg(
        long,
        conflicts_with = "config_file",
        conflicts_with = "monitor_tls_certificate",
        requires = "monitor_tls_key_path",
        env = "OMNIS_BOUNCER_MONITOR_TLS_CERTIFICATE"
    )]
    pub monitor_tls_certificate_path: Option<String>,

    /// Name to use for the cookie that stores the queue unique identifier
    #[arg(
        long,
        conflicts_with = "config_file",
        default_value = "omnis-bouncer-id",
        env = "OMNIS_BOUNCER_COOKIE_ID_NAME"
    )]
    pub id_cookie_name: String,

    /// Name to use for the cookie that stores the queue position
    #[arg(
        long,
        conflicts_with = "config_file",
        default_value = "omnis-bouncer-queue-position",
        env = "OMNIS_BOUNCER_COOKIE_POSITION_NAME"
    )]
    pub position_cookie_name: String,

    /// Name to use for the cookie that stores the queue size
    #[arg(
        long,
        conflicts_with = "config_file",
        default_value = "omnis-bouncer-queue-size",
        env = "OMNIS_BOUNCER_COOKIE_QUEUE_SIZE_NAME"
    )]
    pub queue_size_cookie_name: String,

    /// Name to use for the header that indicates if the ID for the request should be evicted
    /// from the store
    #[arg(
        long,
        conflicts_with = "config_file",
        default_value = "x-omnis-bouncer-id-evict",
        env = "OMNIS_BOUNCER_UPSTREAM_HTTP_HEADER_ID_EVICT_NAME"
    )]
    pub id_evict_upstream_http_header: String,

    /// Name to use for the header that stores the queue ID sent to Omnis Studio
    #[arg(
        long,
        conflicts_with = "config_file",
        default_value = "x-omnis-bouncer-id",
        env = "OMNIS_BOUNCER_UPSTREAM_HTTP_HEADER_ID_NAME"
    )]
    pub id_upstream_http_header: String,

    /// Name to use for the header that stores the queue position
    #[arg(
        long,
        conflicts_with = "config_file",
        default_value = "x-omnis-bouncer-queue-position",
        env = "OMNIS_BOUNCER_HTTP_HEADER_POSITION_NAME"
    )]
    pub position_http_header: String,

    /// Name to use for the header that stores the queue size
    #[arg(
        long,
        conflicts_with = "config_file",
        default_value = "x-omnis-bouncer-queue-size",
        env = "OMNIS_BOUNCER_HTTP_HEADER_QUEUE_SIZE_NAME"
    )]
    pub queue_size_http_header: String,

    /// Timeout (in seconds) when acquiring a connection from the pool
    #[arg(
        long,
        conflicts_with = "config_file",
        default_value = "10",
        env = "OMNIS_BOUNCER_ACQUIRE_TIMEOUT_SECS"
    )]
    pub acquire_timeout: u64,

    /// Timeout (in seconds) when connecting to an upstream server
    #[arg(
        long,
        conflicts_with = "config_file",
        default_value = "10",
        env = "OMNIS_BOUNCER_CONNECT_TIMEOUT_SECS"
    )]
    pub connect_timeout: u64,

    /// Expiration (in seconds) for the cookie that stores the queue identifier
    #[arg(
        long,
        conflicts_with = "config_file",
        default_value = "86400",
        env = "OMNIS_BOUNCER_COOKIE_ID_EXPIRATION_SECS"
    )]
    pub cookie_id_expiration: u64,

    /// Timeout (in seconds) for sticky sessions to be maintained until they are evicted
    #[arg(
        long,
        conflicts_with = "config_file",
        default_value = "600",
        env = "OMNIS_BOUNCER_STICKY_SESSION_TIMEOUT_SECS"
    )]
    pub sticky_session_timeout: u64,

    /// Timeout (in seconds) for caching assets from upstream servers
    #[arg(
        long,
        conflicts_with = "config_file",
        default_value = "60",
        env = "OMNIS_BOUNCER_ASSET_CACHE_SECS"
    )]
    pub asset_cache_secs: u64,

    /// Number of connections to buffer while instituting rate limiting (if rate limiting enabled)
    #[arg(
        long,
        conflicts_with = "config_file",
        default_value = "1000",
        env = "OMNIS_BOUNCER_BUFFER_CONNECTIONS"
    )]
    pub buffer_connections: usize,

    /// Number of requests per second that can be received for the Javascript Client before being rate limited
    #[arg(
        long,
        conflicts_with = "config_file",
        default_value = "0",
        env = "OMNIS_BOUNCER_JS_CLIENT_RATE_LIMIT_PER_SEC"
    )]
    pub js_client_rate_limit_per_sec: u64,

    /// Number of requests per second that can be received for the API server before being rate limited
    #[arg(
        long,
        conflicts_with = "config_file",
        default_value = "5",
        env = "OMNIS_BOUNCER_API_RATE_LIMIT_PER_SEC"
    )]
    pub api_rate_limit_per_sec: u64,

    /// Number of requests per second that can be received for the Ultra-thin server before being rate limited
    #[arg(
        long,
        conflicts_with = "config_file",
        default_value = "0",
        env = "OMNIS_BOUNCER_ULTRA_THIN_RATE_LIMIT_PER_SEC"
    )]
    pub ultra_rate_limit_per_sec: u64,

    /// HTTP port to listen for requests from the public (they will be upgraded to the HTTPS port)
    #[arg(
        long,
        conflicts_with = "config_file",
        default_value = "3000",
        env = "OMNIS_BOUNCER_PUBLIC_HTTP_PORT"
    )]
    pub public_http_port: u16,

    /// HTTPS port to listen for requests from the public
    #[arg(
        long,
        conflicts_with = "config_file",
        default_value = "3001",
        env = "OMNIS_BOUNCER_PUBLIC_HTTPS_PORT"
    )]
    pub public_https_port: u16,

    /// HTTPS port to listen for requests for monitor and control
    #[arg(
        long,
        conflicts_with = "config_file",
        default_value = "2999",
        env = "OMNIS_BOUNCER_MONITOR_HTTPS_PORT"
    )]
    pub monitor_https_port: u16,

    /// Set the queue to be enabled if starting up and no values are stored in Redis
    #[arg(
        long,
        conflicts_with = "config_file",
        action = ArgAction::Set,
        default_value = "true",
        env = "OMNIS_BOUNCER_QUEUE_ENABLED"
    )]
    pub queue_enabled: bool,

    /// Enable queue rotation at a regular interval, while this server is running.  Only a single
    /// server needs to enable queue rotation if running multiple servers
    #[arg(
        long,
        conflicts_with = "config_file",
        action = ArgAction::Set,
        default_value = "true",
        env = "OMNIS_BOUNCER_QUEUE_ROTATION_ENABLED"
    )]
    pub queue_rotation_enabled: bool,

    /// Set the store capacity to this value if starting up and no values are stored in Redis
    #[arg(
        long,
        conflicts_with = "config_file",
        default_value = "5",
        env = "OMNIS_BOUNCER_STORE_CAPACITY"
    )]
    pub store_capacity: isize,

    /// Prefix to use for all keys in Redis when storing data
    #[arg(
        long,
        conflicts_with = "config_file",
        default_value = "omnis_bouncer",
        env = "OMNIS_BOUNCER_REDIS_PREFIX"
    )]
    pub redis_prefix: String,

    /// Quarantine period (in secs) to allow a user to initiate another request before being evicted
    /// from the queue or store.
    #[arg(
        long,
        conflicts_with = "config_file",
        default_value = "45",
        env = "OMNIS_BOUNCER_QUARANTINE_EXPIRY_SECS"
    )]
    pub quarantine_expiry: u64,

    /// Validated period (in secs) to allow a user to complete their session before evicting them
    /// from the queue or store.
    #[arg(
        long,
        conflicts_with = "config_file",
        default_value = "600",
        env = "OMNIS_BOUNCER_VALIDATED_EXPIRY_SECS"
    )]
    pub validated_expiry: u64,

    /// Maximum frequency (in milliseconds) that the same event will be published to Redis from
    /// this server
    #[arg(
        long,
        conflicts_with = "config_file",
        default_value = "100",
        env = "OMNIS_BOUNCER_PUBLISH_THROTTLE_MILLIS"
    )]
    pub publish_throttle: u64,

    /// Convert headers into arguments for Ultra-Thin requests
    #[arg(
        long,
        conflicts_with = "config_file",
        action = ArgAction::Set,
        default_value = "true",
        env = "OMNIS_BOUNCER_ULTRA_THIN_INJECT_HEADERS"
    )]
    pub ultra_thin_inject_headers: bool,

    /// Omnis Studio library to use as a fallback for requests that aren't routed directly to
    /// /ultra.  Must be used in conjunction with fallback_ultra_thin_class.
    #[arg(
        long,
        conflicts_with = "config_file",
        requires = "fallback_ultra_thin_class",
        env = "OMNIS_BOUNCER_FALLBACK_ULTRA_THIN_LIBRARY"
    )]
    pub fallback_ultra_thin_library: Option<String>,

    /// Omnis Studio remote task class to use as a fallback for requests that aren't routed
    /// directly to /ultra.  Must be used in conjunction with fallback_ultra_thin_library.
    #[arg(
        long,
        conflicts_with = "config_file",
        requires = "fallback_ultra_thin_library",
        env = "OMNIS_BOUNCER_FALLBACK_ULTRA_THIN_CLASS"
    )]
    pub fallback_ultra_thin_class: Option<String>,
}

// Build upstreams from args
fn build_upstream(args: &RunArgs) -> Vec<Upstream> {
    args.upstream
        .iter()
        .map(|u| Upstream::new(u, args.upstream_connections, args.upstream_connections))
        .collect()
}

impl TryFrom<&RunArgs> for Config {
    type Error = Error;
    fn try_from(args: &RunArgs) -> Result<Self> {
        let config = Config {
            app_name: args.name.clone(),
            default_locale: args.default_locale.clone(),
            locales: args
                .locales
                .iter()
                .map(|s| s.to_lowercase())
                .collect::<HashSet<String>>(),
            cookie_secret_key: match &args.cookie_key {
                Some(key) => decode_master_key(key)?,
                None => axum_extra::extract::cookie::Key::generate(),
            },
            redis_uri: args.redis_uri.clone(),
            initial_upstream: build_upstream(args),
            public_tls_pair: build_tls_pair(
                args.public_tls_certificate_path.clone(),
                args.public_tls_key_path.clone(),
                args.public_tls_certificate.clone(),
                args.public_tls_key.clone(),
            )?,
            monitor_tls_pair: build_tls_pair(
                args.monitor_tls_certificate_path.clone(),
                args.monitor_tls_key_path.clone(),
                args.monitor_tls_certificate.clone(),
                args.monitor_tls_key.clone(),
            )?,
            id_cookie_name: args.id_cookie_name.clone(),
            position_cookie_name: args.position_cookie_name.clone(),
            queue_size_cookie_name: args.queue_size_cookie_name.clone(),
            id_upstream_http_header: args.id_upstream_http_header.to_lowercase(), // Must be lowercase
            id_evict_upstream_http_header: args.id_evict_upstream_http_header.to_lowercase(), // Must be lowercase
            position_http_header: args.position_http_header.to_lowercase(), // Must be lowercase
            queue_size_http_header: args.queue_size_http_header.to_lowercase(), // Must be lowercase
            acquire_timeout: Duration::from_secs(args.acquire_timeout),
            connect_timeout: Duration::from_secs(args.connect_timeout),
            cookie_id_expiration: Duration::from_secs(args.cookie_id_expiration),
            sticky_session_timeout: Duration::from_secs(args.sticky_session_timeout),
            asset_cache_secs: Duration::from_secs(args.asset_cache_secs),
            buffer_connections: args.buffer_connections,
            js_client_rate_limit_per_sec: args.js_client_rate_limit_per_sec,
            api_rate_limit_per_sec: args.api_rate_limit_per_sec,
            ultra_rate_limit_per_sec: args.ultra_rate_limit_per_sec,
            http_port: args.public_http_port,
            https_port: args.public_https_port,
            control_port: args.monitor_https_port,
            queue_enabled: args.queue_enabled,
            queue_rotation_enabled: args.queue_rotation_enabled,
            store_capacity: StoreCapacity::try_from(args.store_capacity)?,
            queue_prefix: args.redis_prefix.clone(),
            quarantine_expiry: Duration::from_secs(args.quarantine_expiry),
            validated_expiry: Duration::from_secs(args.validated_expiry),
            publish_throttle: Duration::from_millis(args.publish_throttle),
            ultra_thin_inject_headers: args.ultra_thin_inject_headers,
            fallback_ultra_thin_library: args.fallback_ultra_thin_library.clone(),
            fallback_ultra_thin_class: args.fallback_ultra_thin_class.clone(),
        };

        Ok(config)
    }
}

#[derive(Args)]
pub struct ExportAuthorityArgs {
    #[command(subcommand)]
    pub command: Option<ExportAuthorityCommands>,
}

#[derive(Subcommand)]
pub enum ExportAuthorityCommands {
    /// Export as .pfx in Personal Information Exchange (PFX) format
    Pfx { path: String },
    /// Export as .crt in Privacy-Enhanced Mail (PEM) format
    Pem { path: String },
}

pub fn parse_cli() -> Cli {
    Cli::parse()
}
