use axum::{
    error_handling::HandleErrorLayer, extract::{OriginalUri, Request, State},
    response::IntoResponse,
    routing::{any, get},
    BoxError,
    Router,
};
use axum_response_cache::CacheLayer;
use http::header::{
    ACCEPT, ACCEPT_ENCODING, CONNECTION, CONTENT_ENCODING, CONTENT_LENGTH, CONTENT_TYPE,
    PROXY_AUTHENTICATE, PROXY_AUTHORIZATION, TE, TRAILER, TRANSFER_ENCODING, UPGRADE,
    UPGRADE_INSECURE_REQUESTS,
};
use http::{HeaderMap, HeaderName, HeaderValue, Method, StatusCode};
use lazy_static::lazy_static;
use regex::{Regex, RegexBuilder};
use std::{collections::HashSet, time::Duration};
use tower::{buffer::BufferLayer, limit::RateLimitLayer, load_shed::LoadShedLayer, ServiceBuilder};
use tower_cookies::{Cookie, CookieManagerLayer, Cookies};
use tower_http::{compression::CompressionLayer, decompression::RequestDecompressionLayer};
use tracing::{error, info};

use crate::cookies::add_private_server_cookie;
use crate::errors::Result;
use crate::state::AppState;
use crate::upstream::{ConnectionPermit, UpstreamPool};
use crate::waiting_room::{check_waiting_page, extract_queue_id, QueueId, WaitingRoom};

lazy_static! {
    static ref ULTRA_THIN_IGNORE: HashSet<HeaderName> = {
        let mut set = HashSet::new();
        set.insert(ACCEPT_ENCODING);
        set.insert(CONTENT_LENGTH);
        set.insert(CONTENT_ENCODING);
        set.insert(CONNECTION);
        set.insert(PROXY_AUTHENTICATE);
        set.insert(PROXY_AUTHORIZATION);
        set.insert(TE);
        set.insert(TRAILER);
        set.insert(TRANSFER_ENCODING);
        set.insert(UPGRADE);
        set.insert(UPGRADE_INSECURE_REQUESTS);

        set
    };
    static ref UPSTREAM_IGNORE: HashSet<HeaderName> = {
        let mut set = HashSet::new();
        set.insert(ACCEPT);
        set.insert(ACCEPT_ENCODING);
        set.insert(CONTENT_LENGTH);
        set.insert(CONTENT_ENCODING);
        set.insert(CONNECTION);
        set.insert(PROXY_AUTHENTICATE);
        set.insert(PROXY_AUTHORIZATION);
        set.insert(TE);
        set.insert(TRAILER);
        set.insert(TRANSFER_ENCODING);
        set.insert(UPGRADE);
        set.insert(UPGRADE_INSECURE_REQUESTS);

        set
    };
    static ref FAVICON_RE: Regex = RegexBuilder::new(r"^/favicon.ico$")
        .case_insensitive(true)
        .build()
        .unwrap();
    static ref ASSET_RE: Regex =
        RegexBuilder::new(r"^/jschtml/(css|fonts|icons|images|scripts|themes)/")
            .case_insensitive(true)
            .build()
            .unwrap();
    static ref JSCLIENT_RE: Regex = RegexBuilder::new(r"^/(jschtml|jsclient|push)")
        .case_insensitive(true)
        .build()
        .unwrap();
    static ref RESTAPI_RE: Regex = RegexBuilder::new(r"^/api")
        .case_insensitive(true)
        .build()
        .unwrap();
    static ref ULTRATHIN_RE: Regex = RegexBuilder::new(r"^/ultra")
        .case_insensitive(true)
        .build()
        .unwrap();
    static ref HTML_RE: Regex = RegexBuilder::new(r"\.(htm|html)$")
        .case_insensitive(true)
        .build()
        .unwrap();
}

fn cache_router<T>(state: AppState) -> Router<T> {
    let config = &state.config;

    // Asset cache for any resources that are static and common to all upstream servers
    let asset_cache = CacheLayer::with_lifespan(config.asset_cache_secs).use_stale_on_failure();

    Router::new()
        .route("/favicon.ico", get(omnis_studio_upstream))
        .route("/jschtml/css/{*key}", get(omnis_studio_upstream))
        .route("/jschtml/fonts/{*key}", get(omnis_studio_upstream))
        .route("/jschtml/icons/{*key}", get(omnis_studio_upstream))
        .route("/jschtml/images/{*key}", get(omnis_studio_upstream))
        .route("/jschtml/scripts/{*key}", get(omnis_studio_upstream))
        .route("/jschtml/themes/{*key}", get(omnis_studio_upstream))
        .route_layer(asset_cache)
        .with_state(state.clone())
}

fn api_router<T>(state: AppState) -> Router<T> {
    let config = &state.config;

    let mut api_router = Router::new().route("/api/{*key}", any(omnis_studio_upstream));

    if state.config.api_rate_limit_per_sec > 0 {
        api_router = api_router.route_layer(
            ServiceBuilder::new()
                .layer(HandleErrorLayer::new(|err: BoxError| async move {
                    error!("API Rate limiter error: {}", err);
                    (StatusCode::INTERNAL_SERVER_ERROR, "internal server error")
                }))
                .layer(BufferLayer::new(config.buffer_connections))
                .layer(RateLimitLayer::new(
                    config.api_rate_limit_per_sec,
                    Duration::from_secs(1),
                )),
        );
    }

    api_router.with_state(state.clone())
}

fn ultra_thin_router<T>(state: AppState) -> Router<T> {
    let mut ultra_router = Router::new().route("/ultra", any(omnis_studio_upstream));

    if state.config.ultra_rate_limit_per_sec > 0 {
        ultra_router = ultra_router.route_layer(
            ServiceBuilder::new()
                .layer(HandleErrorLayer::new(|err: BoxError| async move {
                    error!("Ultra-Thin rate limiter error: {}", err);
                    (StatusCode::INTERNAL_SERVER_ERROR, "internal server error")
                }))
                .layer(BufferLayer::new(state.config.buffer_connections))
                .layer(RateLimitLayer::new(
                    state.config.ultra_rate_limit_per_sec,
                    Duration::from_secs(1),
                )),
        );
    }

    ultra_router.with_state(state.clone())
}

fn javascript_client_router<T>(state: AppState) -> Router<T> {
    let mut jsclient_router = Router::new()
        .route("/jschtml/{*key}", any(omnis_studio_upstream))
        .route("/jsclient", any(omnis_studio_upstream))
        .route("/push", any(omnis_studio_upstream));

    if state.config.js_client_rate_limit_per_sec > 0 {
        jsclient_router = jsclient_router.route_layer(
            ServiceBuilder::new()
                .layer(HandleErrorLayer::new(|err: BoxError| async move {
                    error!("JS Client rate limiter error: {}", err);
                    (StatusCode::INTERNAL_SERVER_ERROR, "internal server error")
                }))
                .layer(BufferLayer::new(state.config.buffer_connections))
                .layer(RateLimitLayer::new(
                    state.config.js_client_rate_limit_per_sec,
                    Duration::from_secs(1),
                )),
        );
    }

    jsclient_router.with_state(state.clone())
}

// Build the router for the reverse proxy system
pub fn router(state: AppState) -> Router {
    Router::new()
        .merge(cache_router(state.clone()))
        .merge(javascript_client_router(state.clone()))
        .merge(ultra_thin_router(state.clone()))
        .merge(api_router(state.clone()))
        .fallback(omnis_studio_upstream)
        .with_state(state.clone())
        .layer(CookieManagerLayer::new())
        .layer(RequestDecompressionLayer::new())
        .layer(CompressionLayer::new())
        .layer(
            ServiceBuilder::new()
                .layer(HandleErrorLayer::new(|err: BoxError| async move {
                    error!("load shed error: {}", err);
                    (StatusCode::INTERNAL_SERVER_ERROR, "internal server error")
                }))
                .layer(LoadShedLayer::new()),
        )
}

pub async fn omnis_studio_upstream(
    State(state): State<AppState>,
    method: Method,
    cookies: Cookies,
    headers: HeaderMap,
    uri: OriginalUri,
    request: Request,
) -> Result<impl IntoResponse> {
    // Extract config
    let state = state.clone();
    let config = &state.config;

    // Clone properties of the request that are used
    let path_and_query = uri.path_and_query().unwrap();
    let path = path_and_query.path();

    // Private Cookies
    let private_cookies = cookies.private(&config.cookie_secret_key);

    let connection_type = ConnectionType::new(&method, path);
    if connection_type == ConnectionType::Reject {
        return Ok((
            StatusCode::NOT_FOUND,
            HeaderMap::new(),
            axum::body::Body::from("Not Found"),
        ));
    }

    // Clone headers for use with the upstream
    let mut upstream_headers = headers.clone();

    // Extract cookie values
    let connection_permit = if connection_type.requires_waiting_room() {
        // Extract Queue ID
        let id_cookie = private_cookies.get(config.id_cookie_name.clone().as_str());
        let queue = &state.queue;
        let queue_id = extract_queue_id(queue, &id_cookie);

        // Attach cookie queue ID, if it's new
        if id_cookie.is_none() {
            add_private_server_cookie(
                &private_cookies,
                config.id_cookie_name.clone(),
                String::from(queue_id),
                Some(config.cookie_id_expiration), // 1 day ID expiration
            );
        }

        // Check if the use is in the store
        if let Some((waiting_headers, waiting_body)) =
            check_waiting_page(config, &cookies, queue, queue_id).await?
        {
            return Ok((
                StatusCode::SERVICE_UNAVAILABLE,
                waiting_headers,
                waiting_body,
            ));
        }

        // Add queue_id into the upstream headers
        upstream_headers.insert(
            HeaderName::from_lowercase(config.id_upstream_http_header.as_bytes())?,
            String::from(queue_id).parse()?,
        );

        // Strip waiting room cookies if we've arrived in the store
        cookies.remove(Cookie::from(config.position_cookie_name.clone()));
        cookies.remove(Cookie::from(config.queue_size_cookie_name.clone()));

        get_connection(
            &state.upstream_pool,
            connection_type,
            Some(queue_id),
            config.acquire_timeout,
        )
        .await
    } else {
        get_connection(
            &state.upstream_pool,
            connection_type,
            None,
            config.acquire_timeout,
        )
        .await
    };

    // Process connection permit to determine upstream URI
    let mut upstream_uri = match connection_permit {
        Some(guard) => format!("{}{:?}", guard.uri, path_and_query),
        None => {
            return Ok((
                StatusCode::SERVICE_UNAVAILABLE,
                HeaderMap::new(),
                axum::body::Body::from("Service Unavailable"),
            ));
        }
    };

    // Ultra-thin has special requirements for headers, as they must be appended on to the POST
    // body or GET arguments so that Omnis has access to them
    let body = if is_ultra_thin(path) && config.ultra_thin_inject_headers {
        let content_type = match headers.get(CONTENT_TYPE) {
            Some(content_type) => content_type.to_str()?,
            None => "text/plain",
        };

        let ultra_thin_headers: Vec<String> = upstream_headers
            .iter()
            .filter(|(h, _)| !ULTRA_THIN_IGNORE.contains(*h))
            .map(|(h, v)| {
                format!(
                    "HTTP_{}={}",
                    h.as_str().to_uppercase().replace("-", "_"),
                    urlencoding::encode(v.to_str().unwrap())
                )
            })
            .collect();

        if method == Method::GET {
            upstream_uri = format!("{}&{}", upstream_uri, ultra_thin_headers.join("&"));

            reqwest::Body::wrap_stream(request.into_body().into_data_stream())
        } else if method == Method::POST && content_type == "application/x-www-form-urlencoded" {
            // Remove content length header, so we can modify the POST body (reqwest will figure out the new size)
            upstream_headers.remove(CONTENT_LENGTH);

            // Read the body into a local buffer
            let mut bytes: Vec<u8> = axum::body::to_bytes(request.into_body(), usize::MAX)
                .await?
                .to_vec();

            // Extend the body with the modified headers
            bytes.extend_from_slice("&".as_bytes());
            bytes.extend_from_slice(ultra_thin_headers.join("&").as_bytes());

            reqwest::Body::from(bytes)
        } else {
            reqwest::Body::wrap_stream(request.into_body().into_data_stream())
        }
    } else {
        reqwest::Body::wrap_stream(request.into_body().into_data_stream())
    };

    // Process Request on Upstream
    let client = &state.http_client;
    let response = client
        .request(method.clone(), upstream_uri.clone())
        .headers(upstream_headers)
        .body(body)
        .send()
        .await?;

    // Extract content type -- maybe don't add header for certain types?
    let content_type = match response.headers().get(CONTENT_TYPE) {
        Some(v) => String::from(v.to_str()?),
        None => String::from("<unknown>"),
    };

    info!(
        "{} {} -> {} -> {}",
        method, path_and_query, upstream_uri, content_type
    );

    // Build Headers
    let response_headers: HeaderMap<HeaderValue> = response
        .headers()
        .iter()
        .filter_map(|(k, v)| match UPSTREAM_IGNORE.contains(k) {
            true => None,
            false => Some((k.to_owned(), v.to_owned())),
        })
        .collect();

    // Copy all response headers except the ones in the ignore list
    let response_status = response.status();
    let response_body = axum::body::Body::from_stream(response.bytes_stream());

    Ok((response_status, response_headers, response_body))
}

fn is_static_asset(path: &str) -> bool {
    FAVICON_RE.is_match(path) || ASSET_RE.is_match(path)
}

fn is_javascript_client(path: &str) -> bool {
    JSCLIENT_RE.is_match(path)
}

fn is_rest_api(path: &str) -> bool {
    RESTAPI_RE.is_match(path)
}

fn is_ultra_thin(path: &str) -> bool {
    ULTRATHIN_RE.is_match(path)
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum ConnectionType {
    CacheLoad,
    StickySession,
    Regular(WaitingRoom),
    Reject,
}

impl ConnectionType {
    // Get a connection permit for the request, based on the method and path.
    fn new(method: &Method, path: &str) -> ConnectionType {
        if method == Method::GET && is_static_asset(path) {
            // Static assets get a fast-path, since they will be cached by this server
            ConnectionType::CacheLoad
        } else if is_javascript_client(path) {
            // JS Client gets a special path for sticky session handling
            ConnectionType::StickySession
        } else if is_rest_api(path) {
            // REST APIs always start with /api
            ConnectionType::Regular(WaitingRoom::Skip)
        } else if is_ultra_thin(path) {
            // Ultra-thin can't make any assumptions about the content, so we have to guess
            // that the page will be HTML
            if method == Method::GET {
                ConnectionType::Regular(WaitingRoom::Required)
            } else {
                ConnectionType::Regular(WaitingRoom::Skip)
            }
        } else {
            // All other requests can skip the waiting room, since we don't know what they are
            ConnectionType::Reject
        }
    }

    fn requires_waiting_room(&self) -> bool {
        match self {
            ConnectionType::StickySession => true,
            ConnectionType::Regular(WaitingRoom::Required) => true,
            ConnectionType::Regular(WaitingRoom::Skip) => false,
            ConnectionType::CacheLoad => false,
            ConnectionType::Reject => false,
        }
    }
}

// Get a connection permit for the request, based on the method and path.
pub async fn get_connection(
    pool: &UpstreamPool,
    connection_type: ConnectionType,
    queue_token: Option<QueueId>,
    timeout: Duration,
) -> Option<ConnectionPermit> {
    match connection_type {
        ConnectionType::StickySession => match queue_token {
            Some(id) => {
                pool.acquire_sticky_session_permit(&id.into(), timeout)
                    .await
            }
            None => None,
        },
        ConnectionType::Regular(_) => pool.acquire_connection_permit(timeout).await,
        ConnectionType::CacheLoad => pool.acquire_cache_load_permit().await,
        ConnectionType::Reject => None,
    }
}
