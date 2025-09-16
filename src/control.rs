use axum::extract::State;
use axum::response::IntoResponse;
use axum::{routing::get, Json, Router};
use tower_serve_static::ServeDir;

use crate::constants::STATIC_ASSETS_DIR;
use crate::errors::Result;
use crate::state::AppState;

pub fn router<T>(state: AppState) -> Router<T> {
    // Support static file handling from /static directory that is embedded in the final binary
    let static_service = ServeDir::new(&STATIC_ASSETS_DIR);

    // Reverse proxy app
    Router::new()
        .route("/health", get(health_handler))
        .route("/settings", get(settings_handler))
        .route("/status", get(status_handler))
        .nest_service("/static", static_service)
        .with_state(state.clone())
}

async fn health_handler(State(state): State<AppState>) -> impl IntoResponse {
    "ok"
}

async fn settings_handler(State(state): State<AppState>) -> Result<impl IntoResponse> {
    let state = state.clone();
    let config = &state.config;
    let queue = &state.queue;

    let queue_settings = queue.queue_settings(config.queue_prefix.clone()).await?;
    Ok(Json(queue_settings))
}

async fn status_handler(State(state): State<AppState>) -> Result<impl IntoResponse> {
    let state = state.clone();
    let config = &state.config;
    let queue = &state.queue;

    let queue_status = queue.queue_status(config.queue_prefix.clone()).await?;
    Ok(Json(queue_status))
}
