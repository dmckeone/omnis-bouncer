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

use axum::routing::get;
use axum::Router;
use axum_response_cache::CacheLayer;
use axum_server::tls_rustls::RustlsConfig;
use axum_server::Handle;
use reqwest::Client;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::join;
use tower_cookies::CookieManagerLayer;
use tower_http::{compression::CompressionLayer, decompression::RequestDecompressionLayer};
use tracing::{error, info, Level};

use crate::constants::{SELF_SIGNED_CERT, SELF_SIGNED_KEY};
use crate::database::create_redis_pool;
use crate::queue::QueueControl;
use crate::reverse_proxy::reverse_proxy_handler;
use crate::servers::{redirect_http_to_https, secure_server};
use crate::signals::shutdown_signal;
use crate::state::{AppState, Config};
use crate::upstream::UpstreamPool;

// Testing functions for adding dynamic upstream values
fn test_dynamic_upstreams(state: Arc<AppState>) {
    tokio::task::spawn(async move {
        // Wait 1 second to simulate a user change
        tokio::time::sleep(Duration::from_secs(1)).await;

        info!("Add");
        state
            .upstream_pool
            .add_upstreams(&[
                String::from("http://127.0.0.1:63111"),
                String::from("http://127.0.0.1:63112"),
                String::from("http://127.0.0.1:63113"),
            ])
            .await;
        info!("Pool State: {:?}", state.upstream_pool.current_uris().await);

        tokio::time::sleep(Duration::from_secs(1)).await;

        info!("Remove");
        state
            .upstream_pool
            .remove_upstreams(&[
                String::from("http://127.0.0.1:63111"),
                String::from("http://127.0.0.1:63112"),
                String::from("http://127.0.0.1:63113"),
            ])
            .await;
        info!("Pool State: {:?}", state.upstream_pool.current_uris().await);

        tokio::time::sleep(Duration::from_secs(1)).await;

        info!("Re-add");
        state
            .upstream_pool
            .add_upstreams(&[String::from("http://127.0.0.1:63111")])
            .await;
        info!("Pool State: {:?}", state.upstream_pool.current_uris().await);
    });
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
    let shutdown_future = shutdown_signal(shutdown_handle.clone());

    // Build Config
    let config = Config {
        app_name: String::from("Omnis Bouncer"),
        cookie_name: String::from("omnis_bouncer"),
        header_name: String::from("x-omnis-bouncer").to_lowercase(), // Must be lowercase
        connect_timeout: Duration::from_secs(10),
        asset_cache_secs: Duration::from_secs(60),
        http_port: 3000,
        https_port: 3001,
        control_port: 2999,
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
    let queue = match QueueControl::new(redis) {
        Ok(q) => q,
        Err(e) => {
            error!("Failed to initialize queue: {:?}", e);
            return;
        }
    };

    if let Err(e) = queue.init().await {
        error!("Failed to initialize queue functions: {:?}", e);
        return;
    };

    // Create a new http client pool
    let client = Client::builder()
        // .cookie_store(false) -- Disabled by default, unlesss "cookies" feature enabled
        .connect_timeout(config.connect_timeout)
        .redirect(reqwest::redirect::Policy::none())
        .referer(false)
        .build()
        .expect("Failed to build HTTP client");

    let upstream_pool = UpstreamPool::new();

    // Create our app state
    let state = Arc::new(AppState {
        config,
        queue,
        upstream_pool,
        client,
    });

    // Create apps
    let control_app: Router = control::router(state.clone())
        // .layer(TraceLayer::new_for_http())
        .layer(CookieManagerLayer::new())
        .layer(RequestDecompressionLayer::new())
        .layer(CompressionLayer::new());

    // Cache assets for 60 seconds,  reducing load on backend server
    let asset_cache =
        CacheLayer::with_lifespan(state.config.asset_cache_secs).use_stale_on_failure();

    // Upstream routing
    let upstream_app: Router = Router::new()
        .route("/favicon.ico", get(reverse_proxy_handler))
        .route("/jschtml/css/{*key}", get(reverse_proxy_handler))
        .route("/jschtml/fonts/{*key}", get(reverse_proxy_handler))
        .route("/jschtml/icons/{*key}", get(reverse_proxy_handler))
        .route("/jschtml/images/{*key}", get(reverse_proxy_handler))
        .route("/jschtml/scripts/{*key}", get(reverse_proxy_handler))
        .route("/jschtml/themes/{*key}", get(reverse_proxy_handler))
        .route_layer(asset_cache)
        .fallback(reverse_proxy_handler)
        .with_state(state.clone())
        // .layer(TraceLayer::new_for_http())
        .layer(CookieManagerLayer::new())
        .layer(RequestDecompressionLayer::new())
        .layer(CompressionLayer::new());

    // TODO: Replace with better controls in Control App
    test_dynamic_upstreams(state.clone());

    let tls_config = RustlsConfig::from_pem(SELF_SIGNED_CERT.into(), SELF_SIGNED_KEY.into())
        .await
        .expect("Failed to read TLS certificate and key");

    let upstream_upgrade_addr = SocketAddr::from(([0, 0, 0, 0], state.config.http_port));
    let upstream_addr = SocketAddr::from(([0, 0, 0, 0], state.config.https_port));
    let control_addr = SocketAddr::from(([0, 0, 0, 0], state.config.control_port));

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
        )
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

    info!("Shutdown complete");
}
