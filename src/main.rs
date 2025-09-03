use axum::{
    Router, extract, extract::State, middleware, response::IntoResponse, response::Response,
    routing::get, serve,
};
use axum_reverse_proxy::ReverseProxy;
use serde_json::json;
use tokio::net::TcpListener;
use tower::ServiceBuilder;
use tower_http::{compression::CompressionLayer, decompression::RequestDecompressionLayer};
use tracing::{Level, info};
use tracing_subscriber::FmtSubscriber;

// Our app state type
#[derive(Clone)]
struct AppState {
    app_name: String,
}

/// Inject the state into middleware extensions so it can be used in response handlers and
/// other middleware
async fn wrap_request(
    State(state): State<AppState>,
    req: extract::Request,
    next: middleware::Next,
) -> Response {
    info!(
        "Request - {}: {} {}",
        state.app_name,
        req.method(),
        req.uri()
    );

    // Execute the request
    let resp = next.run(req).await;

    info!("Response - {}: {}", state.app_name, resp.status());

    // Return the response
    resp
}

#[tokio::main]
async fn main() {
    // Initialize tracing
    FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .with_target(false)
        .with_thread_ids(true)
        .with_file(true)
        .with_line_number(true)
        .compact()
        .init();

    // Create our app state
    let state = AppState {
        app_name: "Omnis Bouncer".to_string(),
    };

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
                .layer(middleware::from_fn_with_state(state, wrap_request))
                .layer(RequestDecompressionLayer::new())
                .layer(CompressionLayer::new())
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

async fn root_handler(State(state): State<AppState>) -> impl IntoResponse {
    axum::Json(json!({
        "app": state.app_name
    }))
}
