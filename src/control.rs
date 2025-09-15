use axum::extract::State;
use axum::response::IntoResponse;
use axum::{routing::get, Router};
use serde_json::json;
use tower_serve_static::ServeDir;

use crate::constants::STATIC_ASSETS_DIR;
use crate::state::AppState;

pub fn router<T>(state: AppState) -> Router<T> {
    // Support static file handling from /static directory that is embedded in the final binary
    let static_service = ServeDir::new(&STATIC_ASSETS_DIR);

    // Reverse proxy app
    Router::new()
        .route("/", get(root_handler))
        .route("/info", get(info_handler))
        .nest_service("/static", static_service)
        .with_state(state.clone())
}

async fn root_handler(State(state): State<AppState>) -> impl IntoResponse {
    axum::Json(json!({
        "app": state.config.app_name
    }))
}

async fn info_handler(State(state): State<AppState>) -> impl IntoResponse {
    axum::Json(json!({
        "app": state.config.app_name
    }))
}
