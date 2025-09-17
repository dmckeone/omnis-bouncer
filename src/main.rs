mod background;
mod constants;
mod control;
mod database;
mod errors;
mod queue;
mod reverse_proxy;
mod servers;
mod signals;
mod state;
mod upstream;

use axum::error_handling::HandleErrorLayer;
use axum::routing::{any, get};
use axum::{BoxError, Router};
use axum_extra::extract::cookie::Key as PrivateCookieKey;
use axum_response_cache::CacheLayer;
use axum_server::tls_rustls::RustlsConfig;
use axum_server::Handle;
use base64::{engine::general_purpose::STANDARD, Engine as _};
use http::StatusCode;
use reqwest::Client;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::join;
use tokio::sync::Notify;
use tower::buffer::BufferLayer;
use tower::limit::RateLimitLayer;
use tower::load_shed::LoadShedLayer;
use tower::ServiceBuilder;
use tower_cookies::CookieManagerLayer;
use tower_http::{compression::CompressionLayer, decompression::RequestDecompressionLayer};
use tracing::{error, info, Level};

use crate::background::background_task_loop;
use crate::constants::{SELF_SIGNED_CERT, SELF_SIGNED_KEY};
use crate::database::create_redis_pool;
use crate::queue::{QueueControl, StoreCapacity};
use crate::reverse_proxy::reverse_proxy_handler;
use crate::servers::{redirect_http_to_https, secure_server};
use crate::signals::shutdown_signal;
use crate::state::{AppState, Config};
use crate::upstream::{Upstream, UpstreamPool};

// Testing functions for adding dynamic upstream values
fn test_dynamic_upstreams(state: AppState) {
    tokio::task::spawn(async move {
        state
            .upstream_pool
            .add_upstreams(&[Upstream::new(
                String::from("http://127.0.0.1:63111"),
                100,
                10,
            )])
            .await;
        info!("Pool State: {:?}", state.upstream_pool.current_uris().await);
    });
}

fn decode_key(master_key: impl Into<String>) -> anyhow::Result<PrivateCookieKey> {
    let master_key = master_key.into();
    match STANDARD.decode(master_key) {
        Ok(k) => Ok(PrivateCookieKey::derive_from(k.as_slice())),
        Err(error) => Err(error.into()),
    }
}

#[tokio::main]
async fn main() {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_max_level(Level::INFO)
        .with_target(false)
        .compact()
        .init();

    // Install crypto provider guard (must be early in app startup)
    rustls::crypto::aws_lc_rs::default_provider()
        .install_default()
        .expect("Failed to install aws_lc_rs for crypto provider");

    // Create a shutdown handle for graceful shutdown  (must be early in app startup)
    let shutdown_handle = Handle::new();
    let shutdown_notify = Arc::new(Notify::new());
    let shutdown_future = shutdown_signal(shutdown_handle.clone(), shutdown_notify.clone());

    // Build Config

    // TODO: Move cookie master key into configuration parsing
    let base64_master_key =
        "Fkm+v0BDS+XoGNTlfsjLoH97DtqsQL4L2KFB8OkWxk/izMiXgfTE1IoY8MxG7ANYuXCFkpUFstD33Rhq/w03vQ==";

    let cookie_secret_key = match decode_key(base64_master_key) {
        Ok(k) => k,
        Err(error) => {
            error!("Failed to decode cookie master key: {}", error);
            return;
        }
    };

    let config = Config {
        app_name: String::from("Omnis Bouncer"),
        cookie_secret_key,
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
        js_client_rate_limit_per_sec: 100,
        api_rate_limit_per_sec: 1,
        ultra_rate_limit_per_sec: 2,
        http_port: 3000,
        https_port: 3001,
        control_port: 2999,
        queue_enabled: true,
        store_capacity: StoreCapacity::Sized(5),
        queue_prefix: String::from("omnis_bouncer"),
        quarantine_expiry: Duration::from_secs(45),
        validated_expiry: Duration::from_secs(600),
    };

    // Create Redis Pool
    let redis = match create_redis_pool("redis://127.0.0.1") {
        Ok(r) => r,
        Err(e) => {
            error!("Failed to connect to redis: {:?}", e);
            return;
        }
    };

    // Create queue control and initialize functions
    let queue = match QueueControl::new(redis, config.quarantine_expiry, config.validated_expiry) {
        Ok(q) => q,
        Err(e) => {
            error!("Failed to initialize queue: {:?}", e);
            return;
        }
    };

    // Initialize queue functions
    if let Err(e) = queue.init().await {
        error!("Failed to initialize queue functions: {:?}", e);
        return;
    };

    // Initialize basic prefix keys, if not yet setup
    if let Ok(has_keys) = queue.check_sync_keys(config.queue_prefix.clone()).await {
        let result = queue
            .set_queue_settings(
                config.queue_prefix.clone(),
                config.queue_enabled,
                config.store_capacity,
            )
            .await;

        if !has_keys && let Err(e) = result {
            error!("Failed to initialize queue functions: {:?}", e);
            return;
        }
    }

    // Create a new http client pool
    let client = Client::builder()
        // .cookie_store(false) -- Disabled by default, unlesss "cookies" feature enabled
        .connect_timeout(config.connect_timeout)
        .redirect(reqwest::redirect::Policy::none())
        .referer(false)
        .build()
        .expect("Failed to build HTTP client");

    let upstream_pool = UpstreamPool::new(config.sticky_session_timeout);

    // Create our app state
    let state = AppState::new(config, queue, upstream_pool, client);

    // Create apps
    let control_app = build_control_app(&state);
    let upstream_app = build_upstream_app(&state);

    // TODO: Replace with better controls in Control App
    test_dynamic_upstreams(state.clone());

    let tls_config = RustlsConfig::from_pem(SELF_SIGNED_CERT.into(), SELF_SIGNED_KEY.into())
        .await
        .expect("Failed to read TLS certificate and key");

    let upstream_upgrade_addr = SocketAddr::from(([0, 0, 0, 0], state.config.http_port));
    let upstream_addr = SocketAddr::from(([0, 0, 0, 0], state.config.https_port));
    let control_addr = SocketAddr::from(([0, 0, 0, 0], state.config.control_port));

    let background_future = background_task_loop(state.clone(), shutdown_notify.clone());

    info!(
        "HTTP Server running on http://{}:{}",
        upstream_upgrade_addr.ip(),
        upstream_upgrade_addr.port()
    );
    info!(
        "HTTPS Server running on https://{}:{}",
        upstream_addr.ip(),
        upstream_addr.port()
    );
    info!(
        "HTTPS Control Server running on https://{}:{}",
        control_addr.ip(),
        control_addr.port()
    );

    let exit = join!(
        shutdown_future,
        secure_server(
            upstream_addr,
            tls_config.clone(),
            shutdown_handle.clone(),
            upstream_app
        ),
        secure_server(
            control_addr,
            tls_config.clone(),
            shutdown_handle.clone(),
            control_app
        ),
        redirect_http_to_https(
            upstream_upgrade_addr,
            upstream_addr.port(),
            shutdown_handle.clone(),
        ),
        background_future
    );

    // Exit results (ignored)
    if exit.0.is_err() {
        error!("Failed to exit signal handler");
    }
    if exit.1.is_err() {
        error!("Failed to exit upstream server");
    }
    if exit.2.is_err() {
        error!("Failed to exit control server");
    }
    if exit.3.is_err() {
        error!("Failed to exit redirect server");
    }
    if exit.4.is_err() {
        error!("Failed to exit background task loop");
    }

    info!("Shutdown complete");
}

// Build app for controlling the configuration of this server
fn build_control_app(state: &AppState) -> Router {
    control::router(state.clone())
        .layer(CookieManagerLayer::new())
        .layer(RequestDecompressionLayer::new())
        .layer(CompressionLayer::new())
}

// Build the router for the reverse proxy system
fn build_upstream_app(state: &AppState) -> Router {
    // Asset cache for any resources that are static and common to all upstream servers
    let asset_cache =
        CacheLayer::with_lifespan(state.config.asset_cache_secs).use_stale_on_failure();

    // Global rate limiter for API requests
    let api_rate_limit_layer = ServiceBuilder::new()
        .layer(HandleErrorLayer::new(|err: BoxError| async move {
            error!("API Rate limiter error: {}", err);
            (StatusCode::INTERNAL_SERVER_ERROR, "internal server error")
        }))
        .layer(BufferLayer::new(state.config.buffer_connections))
        .layer(RateLimitLayer::new(
            state.config.api_rate_limit_per_sec,
            Duration::from_secs(1),
        ));

    // Global rate limiter for Ultra-Thin requests
    let ultra_rate_limit_layer = ServiceBuilder::new()
        .layer(HandleErrorLayer::new(|err: BoxError| async move {
            error!("Ultra-Thin rate limiter error: {}", err);
            (StatusCode::INTERNAL_SERVER_ERROR, "internal server error")
        }))
        .layer(BufferLayer::new(state.config.buffer_connections))
        .layer(RateLimitLayer::new(
            state.config.ultra_rate_limit_per_sec,
            Duration::from_secs(1),
        ));

    // Global rate limiter for Javascript Client requests
    let jsclient_rate_limit_layer = ServiceBuilder::new()
        .layer(HandleErrorLayer::new(|err: BoxError| async move {
            error!("JS Client rate limiter error: {}", err);
            (StatusCode::INTERNAL_SERVER_ERROR, "internal server error")
        }))
        .layer(BufferLayer::new(state.config.buffer_connections))
        .layer(RateLimitLayer::new(
            state.config.js_client_rate_limit_per_sec,
            Duration::from_secs(1),
        ));

    // Upstream routing
    Router::new()
        .merge(
            Router::new()
                .route("/favicon.ico", get(reverse_proxy_handler))
                .route("/jschtml/css/{*key}", get(reverse_proxy_handler))
                .route("/jschtml/fonts/{*key}", get(reverse_proxy_handler))
                .route("/jschtml/icons/{*key}", get(reverse_proxy_handler))
                .route("/jschtml/images/{*key}", get(reverse_proxy_handler))
                .route("/jschtml/scripts/{*key}", get(reverse_proxy_handler))
                .route("/jschtml/themes/{*key}", get(reverse_proxy_handler))
                .route_layer(asset_cache)
                .with_state(state.clone()),
        )
        .merge(
            Router::new()
                .route("/jschtml/{*key}", any(reverse_proxy_handler))
                .route("/jsclient", any(reverse_proxy_handler))
                .route_layer(jsclient_rate_limit_layer)
                .with_state(state.clone()),
        )
        .merge(
            Router::new()
                .route("/ultra", any(reverse_proxy_handler))
                .route_layer(ultra_rate_limit_layer)
                .with_state(state.clone()),
        )
        .merge(
            Router::new()
                .route("/api/{*key}", any(reverse_proxy_handler))
                .route_layer(api_rate_limit_layer)
                .with_state(state.clone()),
        )
        .fallback(reverse_proxy_handler)
        .with_state(state.clone())
        .layer(CookieManagerLayer::new())
        .layer(RequestDecompressionLayer::new())
        .layer(CompressionLayer::new())
        .layer(
            ServiceBuilder::new()
                .layer(HandleErrorLayer::new(|err: BoxError| async move {
                    error!("service error: {}", err);
                    (StatusCode::INTERNAL_SERVER_ERROR, "internal server error")
                }))
                .layer(LoadShedLayer::new()),
        )
}
