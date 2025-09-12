mod api;
mod constants;
mod database;
mod errors;
mod queue;
mod reverse_proxy;
mod state;
mod upstream;

use axum::{serve, Router};
use reqwest::Client;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpListener;
use tower_cookies::CookieManagerLayer;
use tower_http::{
    compression::CompressionLayer, decompression::RequestDecompressionLayer, trace::TraceLayer,
};
use tower_serve_static::ServeDir;
use tracing::{error, info, Level};
use tracing_subscriber::FmtSubscriber;

use crate::constants::STATIC_ASSETS_DIR;
use crate::database::create_redis_pool;
use crate::queue::QueueControl;
use crate::reverse_proxy::reverse_proxy_handler;
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
        .with_max_level(Level::DEBUG)
        .with_target(false)
        .compact()
        .init();

    // Build Config
    let config = Config {
        app_name: String::from("Omnis Bouncer"),
        cookie_name: String::from("omnis_bouncer"),
        header_name: String::from("x-omnis-bouncer").to_lowercase(), // Must be lowercase
        connect_timeout: Duration::from_secs(10),
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

    // Support static file handling from /static directory that is embedded in the final binary
    let static_service = ServeDir::new(&STATIC_ASSETS_DIR);

    // Create our main router with app state
    let app = Router::new();

    let app = app
        .nest("/_omnisbouncer", api::router(state.clone()))
        .nest_service("/_omnisbouncer/static", static_service);

    // Add fallback
    let app = app.fallback(reverse_proxy_handler);

    // Add state (must be last)
    let app = app.with_state(state.clone());

    // Add utility layers
    let app = app
        .layer(CookieManagerLayer::new())
        .layer(RequestDecompressionLayer::new())
        .layer(CompressionLayer::new());
    // .layer(
    //     // https://github.com/tokio-rs/axum/blob/main/examples/tracing-aka-logging/src/main.rs
    //     TraceLayer::new_for_http(),
    // );

    test_dynamic_upstreams(state.clone());

    // Create a TCP listener
    let listener = TcpListener::bind("0.0.0.0:3000").await.unwrap();
    info!("Server running on http://localhost:3000");
    info!("Try:");
    info!("  - GET /                -> Reverse Proxy");
    info!("  - GET /_omnisbouncer   -> Omnis Bouncer control");
    info!("");
    info!("Example curl commands:");
    info!("  curl http://127.0.0.1:3000/");
    info!("  curl http://1270.0.0.1:3000/_omnisbouncer");
    info!("  curl http://1270.0.0.1:3000/_omnisbouncer/static/focus.jpg");

    // Run the server
    serve(listener, app).await.unwrap();
}
