use axum::extract;
use axum::response::IntoResponse;
use axum::{Router, routing::get};
use serde_json::json;

use crate::state::AppState;

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/", get(root_handler))
        .route("/info", get(info_handler))
        .with_state(state)
}

async fn root_handler(extract::State(state): extract::State<AppState>) -> impl IntoResponse {
    axum::Json(json!({
        "app": state.config.app_name
    }))
}

async fn info_handler(extract::State(state): extract::State<AppState>) -> impl IntoResponse {
    axum::Json(json!({
        "app": state.config.app_name
    }))
}
