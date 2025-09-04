mod errors;
mod state;

use crate::errors::Result;
use crate::state::{AppState, Config};
use axum::http::HeaderName;
use axum::http::header::CONTENT_TYPE;
use axum::{
    Router, extract, middleware, response::IntoResponse, response::Response, routing::get, serve,
};
use axum_reverse_proxy::ReverseProxy;
use serde_json::json;
use std::sync::Arc;
use tokio::net::TcpListener;
use tower::ServiceBuilder;
use tower_cookies::{Cookie, CookieManagerLayer, Cookies};
use tower_http::{compression::CompressionLayer, decompression::RequestDecompressionLayer};
use tracing::{Level, info};
use tracing_subscriber::FmtSubscriber;

/// Inject the state into middleware extensions so it can be used in response handlers and
/// other middleware
async fn wrap_request(
    extract::State(state): extract::State<AppState>,
    cookies: Cookies,
    req: extract::Request,
    next: middleware::Next,
) -> Result<Response> {
    let cookie_name = state.config.cookie_name.clone();

    let cookie = cookies.get(&cookie_name);
    let method = req.method().clone();
    let uri = req.uri().clone();

    // Extract queue token from cookie
    let queue_token = match cookie.clone() {
        Some(c) => String::from(c.value()),
        None => String::from("test-queue-id"),
    };

    // Process request
    let mut resp = next.run(req).await;

    // Attach return cookie
    if cookie.is_none() {
        info!("Set cookie");
        let mut cookie = Cookie::new(cookie_name, queue_token.clone());
        cookie.set_http_only(true);
        cookie.set_path("/");
        cookies.add(cookie);
    }

    // Extract content type -- maybe don't add header for certain types?
    let content_type = match resp.headers().get(CONTENT_TYPE) {
        Some(v) => String::from(v.to_str()?),
        None => String::from("<unknown>"),
    };

    // Attach matching header (if REST API)
    let header_name = state.config.header_name.clone();
    resp.headers_mut().insert(
        HeaderName::from_lowercase(header_name.as_bytes())?,
        queue_token.clone().parse()?,
    );

    info!("{} {} -> {} {}", method, uri, resp.status(), content_type);
    Ok(resp)
}

#[tokio::main]
async fn main() {
    // Initialize tracing
    FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .with_target(true)
        .compact()
        .init();

    // Build Config
    let config = Arc::new(Config {
        app_name: String::from("Omnis Bouncer"),
        cookie_name: String::from("omnis_bouncer"),
        header_name: String::from("x-omnis-bouncer"), // Must be lower case
    });

    // Create our app state
    let state = AppState { config };

    // TODO: Look at load balancing: https://github.com/tom-lubenow/axum-reverse-proxy/blob/main/src/balanced_proxy.rs

    // Create a reverse proxy that forwards requests to httpbin.org
    let proxy = ReverseProxy::new("/", "http://127.0.0.1:63111");

    // Create our main router with app state
    let app = Router::new()
        .route("/_omnisbouncer", get(root_handler))
        .with_state(state.clone());

    // Convert proxy to router
    let proxy_router: Router = proxy.into();

    // Add middleware stack
    let app = app.merge(
        proxy_router.layer(
            ServiceBuilder::new()
                .layer(CookieManagerLayer::new())
                .layer(middleware::from_fn_with_state(state.clone(), wrap_request))
                .layer(RequestDecompressionLayer::new())
                .layer(CompressionLayer::new()),
        ),
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

    // Run the server
    serve(listener, app).await.unwrap();
}

async fn root_handler(extract::State(state): extract::State<AppState>) -> impl IntoResponse {
    axum::Json(json!({
        "app": state.config.app_name
    }))
}
