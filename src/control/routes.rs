use axum::{
    extract::State, response::{
        sse::{Event as SSEvent, KeepAlive, Sse},
        Response,
    },
    Json,
    Router,
};
use futures_util::stream::Stream;
use http::{header::CONTENT_TYPE, HeaderValue, StatusCode};
use serde::{Deserialize, Serialize};
use std::convert::Infallible;
use tokio_stream::StreamExt;
use tower_cookies::CookieManagerLayer;
use tower_http::{compression::CompressionLayer, decompression::RequestDecompressionLayer};
use tower_serve_static::{File, ServeDir, ServeFile};
use utoipa::{
    openapi::extensions::ExtensionsBuilder, openapi::tag::TagBuilder, openapi::Tag, OpenApi,
};
use utoipa_axum::{router::OpenApiRouter, routes};
use utoipa_redoc::{Redoc, Servable};
use utoipa_swagger_ui::SwaggerUi;

use crate::constants::{STATIC_ASSETS_DIR, UI_ASSET_DIR, UI_FAVICON, UI_INDEX};
use crate::control::models::{Config, Event, Settings, SettingsPatch, Status};
use crate::errors::Result;
use crate::queue::StoreCapacity;
use crate::signals::cancellable;
use crate::state::AppState;

#[cfg(debug_assertions)]
use crate::constants::LOCALHOST_CORS_DEBUG_URI;
#[cfg(debug_assertions)]
use http::Method;
#[cfg(debug_assertions)]
use tower_http::cors::CorsLayer;

#[derive(OpenApi)]
#[openapi(info(
    title = "Omnis Bouncer",
    description = "Omnis Bouncer",
    license(name = "MIT")
))]
pub struct ControlAPI;

#[derive(Serialize, Deserialize, Clone, Debug)]
struct TagGroup {
    name: String,
    tags: Vec<String>,
}

impl TagGroup {
    fn new(name: impl Into<String>, tags: Vec<impl Into<String>>) -> Self {
        let name = name.into();
        let tags: Vec<String> = tags.into_iter().map(|tag| tag.into()).collect();
        Self { name, tags }
    }
}

fn build_tag(
    name: impl Into<String>,
    display_name: impl Into<String>,
    description: impl Into<String>,
) -> Tag {
    let name = name.into();
    let display_name = display_name.into();
    let description = description.into();

    let mut tag = TagBuilder::new()
        .name(name)
        .description(Some(description))
        .build();

    let extensions = tag
        .extensions
        .get_or_insert(ExtensionsBuilder::new().build());

    extensions.insert(String::from("x-displayName"), display_name.into());

    tag
}

pub fn router(state: AppState) -> Router {
    // Support static file handling from /static directory that is embedded in the final binary
    let static_service = ServeDir::new(&STATIC_ASSETS_DIR);

    // Support Web UI assets from /assets directory
    let favicon_service = ServeFile::new(File::new(
        UI_FAVICON,
        HeaderValue::from_str("image/x-icon").expect("Failed to parse content type"),
    ));
    let asset_service = ServeDir::new(&UI_ASSET_DIR);

    // Create OpenAPI instance
    let openapi = ControlAPI::openapi();

    // Create OpenAPI router with catalogued routes
    #[allow(unused_mut)]
    let openapi_router = OpenApiRouter::with_openapi(openapi)
        .routes(routes!(get_health))
        .routes(routes!(get_config))
        .routes(routes!(get_status))
        .routes(routes!(get_settings, patch_settings))
        .routes(routes!(get_server_sent_events))
        .nest_service("/favicon.ico", favicon_service)
        .nest_service("/static", static_service)
        .nest_service("/assets", asset_service);

    // Split newly created routes into the axum::Router and OpenApi parts
    let (mut router, mut api) = openapi_router.split_for_parts();

    // Add all tags and tag groups
    api.tags = Some(vec![
        build_tag(
            "server",
            "Server",
            "Routes for the running state of the server",
        ),
        build_tag("queue", "Queue", "Routes for queue management and control"),
        build_tag(
            "stream",
            "Streams",
            "Routes for streaming data from the server",
        ),
        build_tag("ops", "Operations", "Routes for simpler DevOps"),
    ]);

    let extensions = api
        .extensions
        .get_or_insert(ExtensionsBuilder::new().build());

    let queue_groups = [
        TagGroup::new("server", ["server"].to_vec()),
        TagGroup::new("queue", ["queue"].to_vec()),
        TagGroup::new("stream", ["stream"].to_vec()),
        TagGroup::new("ops", ["ops"].to_vec()),
    ];
    extensions.insert(
        String::from("x-tagGroup"),
        serde_json::to_value(queue_groups).unwrap(),
    );

    // Merge OpenAPI spec into routing
    router = router
        .merge(SwaggerUi::new("/swagger").url("/openapi.json", api.clone()))
        .merge(Redoc::with_url("/redoc", api))
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

#[utoipa::path(
    get,
    path = "/health",
    tag = "ops",
    summary = "Health",
    description = "Health check for use with telemetry and containers",
    responses(
        (status = 200, description = "OK", body = String, example = "ok")
    )
)]
async fn get_health() -> String {
    String::from("ok")
}

#[utoipa::path(
    get,
    path = "/api/config",
    tag = "server",
    summary = "Server Configuration",
    description = "General configuration of the server",
    responses(
        (status = 200, description = "OK", body = Config)
    )
)]
async fn get_config(State(state): State<AppState>) -> Result<Json<Config>> {
    let state = state.clone();
    let config = &state.config;
    let config = Config::from(config);
    Ok(Json(config))
}

#[utoipa::path(
    get,
    path = "/api/status",
    tag = "queue",
    summary = "Queue Status",
    description = "Status of the queue, combined with settings for easy access",
    responses(
        (status = 200, description = "OK", body = Status)
    )
)]
async fn get_status(State(state): State<AppState>) -> Result<Json<Status>> {
    let state = state.clone();
    let config = &state.config;
    let queue = &state.queue;

    let queue_status = queue.queue_status(config.queue_prefix.clone()).await?;
    Ok(Json(Status::from(queue_status)))
}

#[utoipa::path(
    get,
    path = "/api/settings",
    tag = "queue",
    summary = "Queue Settings",
    description = "Queue settings currently in use",
    responses(
        (status = 200, description = "OK", body = Settings)
    )
)]
async fn get_settings(State(state): State<AppState>) -> Result<Json<Settings>> {
    let state = state.clone();
    let config = &state.config;
    let queue = &state.queue;

    let queue_settings = queue.queue_settings(config.queue_prefix.clone()).await?;
    Ok(Json(Settings::from(queue_settings)))
}

#[utoipa::path(
    patch,
    path = "/api/settings",
    tag = "queue",
    summary = "Queue Settings",
    description = "Alter queue settings.  Missing keys and nil are treated equally, and do not affect the current running state.",
    request_body = SettingsPatch,
    responses(
        (status = 200, description = "OK", body = Settings)
    )
)]
async fn patch_settings(
    State(state): State<AppState>,
    Json(changes): Json<SettingsPatch>,
) -> Result<Json<Settings>> {
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

#[utoipa::path(
    get,
    path = "/api/sse",
    tag = "stream",
    summary = "Server-Sent Events (SSE)",
    description = "Events pushed from the server to notify UI changes.  All payloads are strings:
* `settings:updated`
* `waiting_page:updated`
* `queue:added`
* `queue:expired`
* `store:added`
* `store:expired`
* `queue:removed`

See [MDN - Using Server Sent Events](https://developer.mozilla.org/en-US/docs/Web/API/Server-sent_events/Using_server-sent_events) for more details\
    ",
    responses(
        (status = 200, description = "OK", body = String, example = "settings:updated")
    )
)]
async fn get_server_sent_events(
    State(state): State<AppState>,
) -> Sse<impl Stream<Item = core::result::Result<SSEvent, Infallible>>> {
    let state = state.clone();

    // Create stream of Queue events
    let subscriber = state.queue_events.clone();
    let stream = subscriber.into_stream();

    // Translate into a public facing API and create Serve Sent Event
    let sse_stream = stream
        .map(|ev| String::from(Event::from(ev)))
        .map(|ev| Ok(SSEvent::default().data(ev)));

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
