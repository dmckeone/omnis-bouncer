use axum::extract::State;
use axum::response::IntoResponse;
use axum::{routing::get, Router};
use serde_json::json;
use std::sync::Arc;

use crate::state::AppState;

pub fn router<T>(state: Arc<AppState>) -> Router<T> {
    Router::new()
        .route("/", get(root_handler))
        .route("/info", get(info_handler))
        .with_state(state)
}

async fn root_handler(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    axum::Json(json!({
        "app": state.config.app_name
    }))
}

async fn info_handler(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    axum::Json(json!({
        "app": state.config.app_name
    }))
}
