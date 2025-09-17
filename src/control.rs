use axum::extract::State;
use axum::response::{IntoResponse, Response};
use axum::{routing::get, Json, Router};
use http::header::CONTENT_TYPE;
use http::{HeaderValue, StatusCode};
use tower_serve_static::{File, ServeDir, ServeFile};

use crate::constants::{STATIC_ASSETS_DIR, UI_ASSET_DIR, UI_FAVICON, UI_INDEX};
use crate::errors::Result;
use crate::state::AppState;

pub fn router<T>(state: AppState) -> Router<T> {
    // Support static file handling from /static directory that is embedded in the final binary
    let static_service = ServeDir::new(&STATIC_ASSETS_DIR);

    // Support Web UI assets from /assets directory
    let favicon_service = ServeFile::new(File::new(
        UI_FAVICON,
        HeaderValue::from_str("image/x-icon").expect("Failed to parse content type"),
    ));
    let asset_service = ServeDir::new(&UI_ASSET_DIR);

    // Reverse proxy app
    Router::new()
        .route("/api/health", get(health_handler))
        .route("/api/settings", get(settings_handler))
        .route("/api/status", get(status_handler))
        .nest_service("/favicon.ico", favicon_service)
        .nest_service("/static", static_service)
        .nest_service("/assets", asset_service)
        .fallback(control_ui_handler)
        .with_state(state.clone())
}

async fn health_handler() -> impl IntoResponse {
    "ok"
}

// Current state of Queue Settings
async fn settings_handler(State(state): State<AppState>) -> Result<impl IntoResponse> {
    let state = state.clone();
    let config = &state.config;
    let queue = &state.queue;

    let queue_settings = queue.queue_settings(config.queue_prefix.clone()).await?;
    Ok(Json(queue_settings))
}

// Current State of Queue Status
async fn status_handler(State(state): State<AppState>) -> Result<impl IntoResponse> {
    let state = state.clone();
    let config = &state.config;
    let queue = &state.queue;

    let queue_status = queue.queue_status(config.queue_prefix.clone()).await?;
    Ok(Json(queue_status))
}

// Fallback handler for the Control UI Single Page Application (SPA)
async fn control_ui_handler() -> Result<Response<axum::body::Body>> {
    let response = Response::builder()
        .status(StatusCode::OK)
        .header(CONTENT_TYPE, "text/html; charset=utf-8")
        .body(axum::body::Body::from(UI_INDEX))?;

    Ok(response)
}
