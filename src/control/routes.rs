use axum::extract::State;
use axum::response::sse::{Event as SSEvent, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::{routing::get, Json, Router};
use futures_util::stream::Stream;
use http::header::CONTENT_TYPE;
use http::{HeaderValue, StatusCode};
use std::convert::Infallible;
use tokio_stream::wrappers::errors::BroadcastStreamRecvError;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt as _;
use tower_serve_static::{File, ServeDir, ServeFile};
use tracing::error;

use crate::constants::{STATIC_ASSETS_DIR, UI_ASSET_DIR, UI_FAVICON, UI_INDEX};
use crate::control::models::{Event, Settings, SettingsPatch, Status};
use crate::errors::Result;
use crate::queue::{QueueEvent, StoreCapacity};
use crate::signals::cancellable;
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
        .route("/health", get(read_health))
        .route("/api/settings", get(read_settings).patch(patch_settings))
        .route("/api/status", get(read_status))
        .route("/api/sse", get(server_sent_events))
        .nest_service("/favicon.ico", favicon_service)
        .nest_service("/static", static_service)
        .nest_service("/assets", asset_service)
        .fallback(control_ui_handler)
        .with_state(state.clone())
}

async fn read_health() -> impl IntoResponse {
    "ok"
}

// Current state of Queue Settings
async fn read_settings(State(state): State<AppState>) -> Result<impl IntoResponse> {
    let state = state.clone();
    let config = &state.config;
    let queue = &state.queue;

    let queue_settings = queue.queue_settings(config.queue_prefix.clone()).await?;
    Ok(Json(Settings::from(queue_settings)))
}

/// Update queue settings (enabled and store capacity).  null and missing are treated equivalently
async fn patch_settings(
    State(state): State<AppState>,
    Json(changes): Json<SettingsPatch>,
) -> Result<impl IntoResponse> {
    let state = state.clone();
    let config = &state.config;
    let queue = &state.queue;

    match (changes.queue_enabled, changes.store_capacity) {
        (Some(enabled), Some(capacity)) => {
            // Setting both queue enabled and store capacity
            queue
                .set_queue_settings(
                    config.queue_prefix.clone(),
                    enabled,
                    StoreCapacity::try_from(capacity)?,
                )
                .await?
        }
        (Some(enabled), None) => {
            // Only setting queue enabled
            queue
                .set_queue_enabled(config.queue_prefix.clone(), enabled)
                .await?
        }
        (None, Some(capacity)) => {
            // Only setting store capacity
            queue
                .set_store_capacity(
                    config.queue_prefix.clone(),
                    StoreCapacity::try_from(capacity)?,
                )
                .await?
        }
        (None, None) => {
            // No changes needed, just skip
        }
    }

    let queue_settings = queue.queue_settings(config.queue_prefix.clone()).await?;
    Ok(Json(Settings::from(queue_settings)))
}

// Current State of Queue Status
async fn read_status(State(state): State<AppState>) -> Result<impl IntoResponse> {
    let state = state.clone();
    let config = &state.config;
    let queue = &state.queue;

    let queue_status = queue.queue_status(config.queue_prefix.clone()).await?;
    Ok(Json(Status::from(queue_status)))
}

/// Translate an incoming queue event into a standard format for events as strings
fn translate_queue_event(
    queue_event: core::result::Result<QueueEvent, BroadcastStreamRecvError>,
) -> Option<core::result::Result<SSEvent, Infallible>> {
    match queue_event {
        Ok(ev) => {
            let event: Event = ev.into();
            let event_string = match serde_json::to_string(&event) {
                Ok(s) => s,
                Err(e) => {
                    error!("Failed to serialize event: {}", e);
                    return None;
                }
            };
            let sse = SSEvent::default().data(event_string);
            Some(Ok(sse))
        }
        Err(e) => {
            error!("Failed to receive queue event: {}", e);
            None
        }
    }
}

/// Stream of events occurring in the server
async fn server_sent_events(
    State(state): State<AppState>,
) -> Sse<impl Stream<Item = core::result::Result<SSEvent, Infallible>>> {
    let state = state.clone();

    // Subscribe to queue updates
    let receiver = state.queue.subscribe();

    // Wrap updates in a stream, and translate into a public facing API
    let stream = BroadcastStream::new(receiver).filter_map(translate_queue_event);

    // Ensure that the stream doesn't prevent the server from shutting down
    let stream = cancellable(stream, state.shutdown_notifier.clone());

    Sse::new(stream).keep_alive(KeepAlive::default())
}

// Fallback handler for the Control UI Single Page Application (SPA)
async fn control_ui_handler() -> Result<Response<axum::body::Body>> {
    let response = Response::builder()
        .status(StatusCode::OK)
        .header(CONTENT_TYPE, "text/html; charset=utf-8")
        .body(axum::body::Body::from(UI_INDEX))?;

    Ok(response)
}
