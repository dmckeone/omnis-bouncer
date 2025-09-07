mod api;
mod constants;
mod discovery;
mod errors;
mod reverse_proxy;
mod state;
mod upstream;

use axum::{middleware, serve, Router};
use axum_reverse_proxy::DiscoverableBalancedProxy;
use hyper_util::client::legacy::{connect::HttpConnector, Client};
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpListener;
use tower_cookies::CookieManagerLayer;
use tower_http::{
    compression::CompressionLayer, decompression::RequestDecompressionLayer, trace::TraceLayer,
};
use tower_serve_static::ServeDir;
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

use crate::constants::STATIC_ASSETS_DIR;
use crate::discovery::UpstreamPoolStream;
use crate::state::{AppState, Config};
use crate::upstream::UpstreamPool;

// Testing functions for adding dynamic upstream values
fn test_dynamic_upstreams(state: &AppState) {
    let state = state.clone();
    tokio::task::spawn(async move {
        // Wait 1 second to simulate a user change
        tokio::time::sleep(Duration::from_secs(1)).await;

        info!("Add");
        state
            .upstream_pool
            .add_upstreams(&vec![
                String::from("http://127.0.0.1:63111"),
                String::from("http://127.0.0.1:63112"),
                String::from("http://127.0.0.1:63113"),
            ])
            .await;

        tokio::time::sleep(Duration::from_secs(1)).await;

        info!("Remove");
        state
            .upstream_pool
            .remove_upstreams(&vec![
                String::from("http://127.0.0.1:63111"),
                String::from("http://127.0.0.1:63112"),
                String::from("http://127.0.0.1:63113"),
            ])
            .await;

        tokio::time::sleep(Duration::from_secs(1)).await;

        info!("Re-add");
        state
            .upstream_pool
            .add_upstreams(&vec![String::from("http://127.0.0.1:63111")])
            .await;

        tokio::time::sleep(Duration::from_secs(1)).await;

        let current_uris = state.upstream_pool.current_uris().await;
        info!("Pool State: {:?}", current_uris)
    });
}

#[tokio::main]
async fn main() {
    // Initialize tracing
    FmtSubscriber::builder()
        .with_max_level(Level::DEBUG)
        .with_target(false)
        .compact()
        .init();

    // Build Config
    let config = Arc::new(Config {
        app_name: String::from("Omnis Bouncer"),
        cookie_name: String::from("omnis_bouncer"),
        header_name: String::from("x-omnis-bouncer").to_lowercase(), // Must be lowercase
    });

    // Create our app state
    let state = AppState {
        config,
        upstream_pool: Arc::new(UpstreamPool::new()),
    };

    // Support static file handling from /static directory that is embedded in the final binary
    let static_service = ServeDir::new(&STATIC_ASSETS_DIR);

    // Create a pool discovery stream for dynamic URI addition and removal
    let pool_stream = UpstreamPoolStream::new(state.upstream_pool.clone());

    // Create an HTTP client
    let mut connector = HttpConnector::new();
    connector.set_nodelay(true);
    connector.enforce_http(false);
    connector.set_keepalive(Some(std::time::Duration::from_secs(60)));
    connector.set_connect_timeout(Some(std::time::Duration::from_secs(10)));
    connector.set_reuse_address(true);

    let client = Client::builder(hyper_util::rt::TokioExecutor::new())
        .pool_idle_timeout(Duration::from_secs(60))
        .pool_max_idle_per_host(32)
        .retry_canceled_requests(true)
        .set_host(true)
        .build(connector);

    // Create proxy, kickstart discovery, and return to immutable
    let mut proxy = DiscoverableBalancedProxy::new_with_client("/", client, pool_stream);
    proxy.start_discovery().await;
    let proxy = proxy;

    // Create the reverse proxy with the queue injection middleware
    let proxy_router: Router = proxy.into();
    let proxy_router = proxy_router.layer(middleware::from_fn_with_state(
        state.clone(),
        reverse_proxy::middleware,
    ));

    // Create our main router with app state
    let app = Router::new().with_state(state.clone());

    let app = app
        .nest("/_omnisbouncer", api::router(state.clone()))
        .nest_service("/_omnisbouncer/static", static_service);

    // Add middleware stack
    let app = app.merge(proxy_router);

    // Add utility layers
    let app = app
        .layer(CookieManagerLayer::new())
        .layer(RequestDecompressionLayer::new())
        .layer(CompressionLayer::new())
        .layer(
            // https://github.com/tokio-rs/axum/blob/main/examples/tracing-aka-logging/src/main.rs
            TraceLayer::new_for_http(),
        );

    test_dynamic_upstreams(&state);

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
