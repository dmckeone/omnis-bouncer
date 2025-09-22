use axum::{
    extract::State, response::{
        sse::{Event as SSEvent, KeepAlive, Sse}, IntoResponse,
        Response,
    },
    routing::get,
    Json,
    Router,
};
use futures_util::stream::Stream;
use http::{header::CONTENT_TYPE, HeaderValue, StatusCode};
use std::convert::Infallible;
use tokio_stream::{
    wrappers::errors::BroadcastStreamRecvError, wrappers::BroadcastStream, StreamExt,
};
use tower_cookies::CookieManagerLayer;
use tower_http::{compression::CompressionLayer, decompression::RequestDecompressionLayer};
use tower_serve_static::{File, ServeDir, ServeFile};
use tracing::error;

use crate::constants::{DEBOUNCE_INTERVAL, STATIC_ASSETS_DIR, UI_ASSET_DIR, UI_FAVICON, UI_INDEX};
use crate::control::models::{Event, Info, Settings, SettingsPatch, Status};
use crate::errors::Result;
use crate::queue::{QueueEvent, StoreCapacity};
use crate::signals::cancellable;
use crate::state::AppState;
use crate::stream::debounce;

#[cfg(debug_assertions)]
use crate::constants::LOCALHOST_CORS_DEBUG_URI;
#[cfg(debug_assertions)]
use http::Method;

#[cfg(debug_assertions)]
use tower_http::cors::CorsLayer;

pub fn router(state: AppState) -> Router {
    // Support static file handling from /static directory that is embedded in the final binary
    let static_service = ServeDir::new(&STATIC_ASSETS_DIR);

    // Support Web UI assets from /assets directory
    let favicon_service = ServeFile::new(File::new(
        UI_FAVICON,
        HeaderValue::from_str("image/x-icon").expect("Failed to parse content type"),
    ));
    let asset_service = ServeDir::new(&UI_ASSET_DIR);

    // Reverse proxy app
    #[allow(unused_mut)]
    let mut router = Router::new()
        .route("/health", get(read_health))
        .route("/api/info", get(read_info))
        .route("/api/settings", get(read_settings).patch(patch_settings))
        .route("/api/status", get(read_status))
        .route("/api/sse", get(server_sent_events))
        .nest_service("/favicon.ico", favicon_service)
        .nest_service("/static", static_service)
        .nest_service("/assets", asset_service)
        .fallback(control_ui_handler);

    #[cfg(debug_assertions)]
    {
        let origins = [LOCALHOST_CORS_DEBUG_URI.parse().unwrap()];
        let cors_layer = CorsLayer::new()
            // allow `GET` and `POST` when accessing the resource
            .allow_methods([Method::GET, Method::PATCH])
            // allow requests from any origin
            .allow_origin(origins);

        router = router.layer(cors_layer);
    }

    router
        .with_state(state.clone())
        .layer(CookieManagerLayer::new())
        .layer(RequestDecompressionLayer::new())
        .layer(CompressionLayer::new())
}

async fn read_health() -> impl IntoResponse {
    "ok"
}

// Current state of Queue Settings
async fn read_info(State(state): State<AppState>) -> Result<impl IntoResponse> {
    let state = state.clone();
    let config = &state.config;
    let info = Info {
        name: config.app_name.clone(),
    };
    Ok(Json(info))
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

/// Filter out any errors from the Broadcast stream and log them, leaving only valid queue events
fn filter_queue_event(
    queue_event: core::result::Result<QueueEvent, BroadcastStreamRecvError>,
) -> Option<QueueEvent> {
    match queue_event {
        Ok(ev) => Some(ev),
        Err(e) => {
            error!("Failed to receive queue event: {}", e);
            None
        }
    }
}

/// Translate an incoming queue event into a standard format for events as strings
fn translate_queue_event(
    queue_event: QueueEvent,
) -> Option<core::result::Result<SSEvent, Infallible>> {
    let event: Event = queue_event.into();
    let event_string: String = event.into();
    let sse = SSEvent::default().data(event_string);
    Some(Ok(sse))
}

/// Stream of events occurring in the server
async fn server_sent_events(
    State(state): State<AppState>,
) -> Sse<impl Stream<Item = core::result::Result<SSEvent, Infallible>>> {
    let state = state.clone();

    // Subscribe to queue updates
    let receiver = state.queue.subscribe();

    // Create Broadcast stream that is infallible
    let broadcast_stream = BroadcastStream::new(receiver).filter_map(filter_queue_event);

    // Deduplicate events across a period of time
    let debounce_stream = debounce(DEBOUNCE_INTERVAL, broadcast_stream);

    // Translate into a public facing API
    let sse_stream = debounce_stream.filter_map(translate_queue_event);

    // Ensure that the stream doesn't prevent the server from shutting down
    let safe_stream = cancellable(sse_stream, state.shutdown_notifier.clone());

    Sse::new(safe_stream).keep_alive(KeepAlive::default())
}

// Fallback handler for the Control UI Single Page Application (SPA)
async fn control_ui_handler() -> Result<Response<axum::body::Body>> {
    let response = Response::builder()
        .status(StatusCode::OK)
        .header(CONTENT_TYPE, "text/html; charset=utf-8")
        .body(axum::body::Body::from(UI_INDEX))?;

    Ok(response)
}
