mod api;
mod constants;
mod errors;
mod reverse_proxy;
mod state;

use axum::{middleware, serve, Router};
use axum_reverse_proxy::ReverseProxy;
use std::sync::Arc;
use tokio::net::TcpListener;
use tower_cookies::CookieManagerLayer;
use tower_http::{
    compression::CompressionLayer, decompression::RequestDecompressionLayer, trace::TraceLayer,
};
use tower_serve_static::ServeDir;
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

use crate::constants::STATIC_ASSETS_DIR;
use crate::state::{AppState, Config};

#[tokio::main]
async fn main() {
    // Initialize tracing
    FmtSubscriber::builder()
        .with_max_level(Level::INFO)
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
    let state = AppState { config };

    // Support static file handling from /static directory that is embedded in the final binary
    let static_service = ServeDir::new(&STATIC_ASSETS_DIR);

    // TODO: Look at load balancing: https://github.com/tom-lubenow/axum-reverse-proxy/blob/main/src/balanced_proxy.rs
    // Create the reverse proxy with the queue injection middleware
    let proxy = ReverseProxy::new("/", "http://127.0.0.1:63111");
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
