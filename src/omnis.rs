use axum::{
    BoxError, Router,
    error_handling::HandleErrorLayer,
    extract::{ConnectInfo, OriginalUri, Request, State},
    response::IntoResponse,
    routing::{any, get},
};
use axum_response_cache::CacheLayer;
use base64::{Engine, engine::general_purpose::STANDARD};
use http::{
    HeaderMap, HeaderName, HeaderValue, Method, StatusCode, Uri,
    header::{
        ACCEPT, ACCEPT_ENCODING, CONNECTION, CONTENT_ENCODING, CONTENT_LENGTH, CONTENT_TYPE,
        PROXY_AUTHENTICATE, PROXY_AUTHORIZATION, TE, TRAILER, TRANSFER_ENCODING, UPGRADE,
        UPGRADE_INSECURE_REQUESTS,
    },
    uri::{PathAndQuery, Scheme},
};
use lazy_static::lazy_static;
use regex::{Regex, RegexBuilder};
use std::time::Instant;
use std::{collections::HashSet, net::SocketAddr, time::Duration, time::SystemTime};
use tower::{ServiceBuilder, buffer::BufferLayer, limit::RateLimitLayer, load_shed::LoadShedLayer};
use tower_cookies::{Cookie, CookieManagerLayer, Cookies};
use tower_http::{compression::CompressionLayer, decompression::RequestDecompressionLayer};
use tracing::{error, info};

use crate::config::Config;
use crate::cookies::add_private_server_cookie;
use crate::errors::Result;
use crate::locales::header_locale;
use crate::state::AppState;
use crate::upstream::{ConnectionPermit, UpstreamPool};
use crate::waiting_room::{QueueId, WaitingRoom, check_waiting_page, extract_queue_id};

lazy_static! {
    static ref UPSTREAM_IGNORE: HashSet<HeaderName> = {
        let mut set = HashSet::new();
        set.insert(ACCEPT_ENCODING);
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
    static ref RESPONSE_IGNORE: HashSet<HeaderName> = {
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

fn fallback_ultra_thin_router<T>(state: AppState) -> Router<T> {
    let mut fallback_router = Router::new().fallback(any(omnis_studio_upstream));

    if state.config.ultra_rate_limit_per_sec > 0 {
        fallback_router = fallback_router.route_layer(
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

    fallback_router.with_state(state.clone())
}

// Build the router for the reverse proxy system
pub fn router(state: AppState) -> Router {
    // Base routing
    let mut router = Router::new()
        .merge(cache_router(state.clone()))
        .merge(javascript_client_router(state.clone()))
        .merge(api_router(state.clone()));

    // Optional routing, based on configuration
    if state.config.fallback_enabled() {
        // Fallback is in place, so all ultra-thin routes go through the same fallback router
        router = router.merge(fallback_ultra_thin_router(state.clone()));
    } else {
        // Fallback is not in place, so only ultra-thin routes are routed
        router = router.merge(ultra_thin_router(state.clone()));
    }

    router
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
    ConnectInfo(connect_info): ConnectInfo<SocketAddr>,
    method: Method,
    cookies: Cookies,
    headers: HeaderMap,
    uri: OriginalUri,
    request: Request,
) -> Result<impl IntoResponse> {
    // Extract config
    let state = state.clone();
    let config = &state.config;
    let queue = &state.queue;
    let upstream_pool = &state.upstream_pool;

    // Clone properties of the request that are used
    let path_and_query = uri.path_and_query().unwrap();
    let path = path_and_query.path();

    // Select locale from Accept-Language
    let locale = header_locale(&headers, &config.locales, &config.default_locale);

    // Private Cookies
    let private_cookies = cookies.private(&config.cookie_secret_key);

    let connection_type = ConnectionType::new(&method, path, config.fallback_enabled());
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
            check_waiting_page(config, &cookies, &locale, queue, queue_id).await?
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
    let upstream_uri = match connection_permit {
        Some(guard) => format!("{}{:?}", guard.uri, path_and_query),
        None => {
            return Ok((
                StatusCode::SERVICE_UNAVAILABLE,
                HeaderMap::new(),
                axum::body::Body::from("Service Unavailable"),
            ));
        }
    };

    // Build request body
    let (upstream_method, upstream_uri, upstream_headers, upstream_body) = build_upstream_request(
        config,
        connect_info,
        request,
        path_and_query,
        upstream_headers,
        upstream_uri,
    )
    .await?;

    // Process Request on Upstream
    let start = Instant::now();
    let client = &state.http_client;
    let response = client
        .request(upstream_method.clone(), upstream_uri.clone())
        .headers(upstream_headers.clone())
        .body(upstream_body)
        .send()
        .await?;

    // Extract content type -- maybe don't add header for certain types?
    let content_type = match response.headers().get(CONTENT_TYPE) {
        Some(v) => String::from(v.to_str()?),
        None => String::from("<unknown>"),
    };

    // Log upstream request
    let log_uri: Uri = upstream_uri.parse()?;
    info!(
        "{} {} -> {}://{}{}{} -> {} ({} ms)",
        method,
        path_and_query,
        log_uri.scheme().unwrap_or(&Scheme::HTTP),
        log_uri.host().unwrap_or("<unknown>"),
        match log_uri.port() {
            Some(port) => format!(":{port}"),
            None => String::new(),
        },
        log_uri.path(),
        content_type,
        Instant::now().duration_since(start).as_millis()
    );

    // Check for queue eviction header
    let evict_header = config.id_evict_upstream_http_header.as_str();
    if response.headers().get(evict_header).is_some() {
        // Upstream has specified that this client should be evicted
        let cookie = private_cookies.get(config.id_cookie_name.clone().as_str());
        if let QueueId::Existing(queue_id) = extract_queue_id(queue, &cookie) {
            // Remove cookie
            private_cookies.remove(Cookie::from(config.id_cookie_name.clone()));
            // Drop sticky session (if it exists)
            upstream_pool.remove_sticky_session(&queue_id).await;
            // Drop from queue (if it exists)
            if let Err(error) = state
                .queue
                .id_remove(&config.queue_prefix, queue_id, None)
                .await
            {
                error!(
                    "Failed to evict ID from the queue - {}: {:?}",
                    queue_id, error
                );
            }
        }
    }

    // Build Headers For Response
    let response_headers: HeaderMap<HeaderValue> = response
        .headers()
        .iter()
        .filter(|(k, _)| *k != evict_header && !UPSTREAM_IGNORE.contains(*k))
        .map(|(k, v)| (k.to_owned(), v.to_owned()))
        .collect();

    // Copy all response headers except the ones in the ignore list
    let response_status = response.status();
    let response_body = axum::body::Body::from_stream(response.bytes_stream());

    Ok((response_status, response_headers, response_body))
}

fn upstream_header_filter(entry: &(&HeaderName, &HeaderValue)) -> bool {
    let (h, _) = entry;
    !UPSTREAM_IGNORE.contains(*h) && !(*h).as_str().to_lowercase().starts_with("sec")
}

fn ultra_thin_header_filter(entry: &(&HeaderName, &HeaderValue)) -> bool {
    let (h, _) = entry;
    !ULTRA_THIN_IGNORE.contains(*h) && !(*h).as_str().to_lowercase().starts_with("sec")
}

/// Create the upstream request from the current request
async fn build_upstream_request(
    config: &Config,
    connect_info: SocketAddr,
    request: Request,
    path_and_query: &PathAndQuery,
    upstream_headers: HeaderMap,
    upstream_uri: String,
) -> Result<(Method, String, HeaderMap, reqwest::Body)> {
    let request_method = request.method();
    let request_headers = request.headers();
    let request_path = path_and_query.path();

    let mut upstream_uri = upstream_uri;
    let mut upstream_headers = upstream_headers.clone();

    let use_fallback = config.fallback_enabled()
        && !is_javascript_client(request_path)
        && !is_rest_api(request_path)
        && !is_static_asset(request_path);

    // Ultra-thin has special requirements for headers, as they must be appended on to the POST
    // body or GET arguments so that Omnis has access to them
    let mut upstream_method = request_method.clone();
    let upstream_body =
        if use_fallback || (is_ultra_thin(request_path) && config.ultra_thin_inject_headers) {
            let content_type = match request_headers.get(CONTENT_TYPE) {
                Some(content_type) => content_type.to_str()?,
                None => "text/plain",
            };

            let epoch = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH)?;
            let mut ultra_thin_info: Vec<String> = vec![
                format!("SERVER_TIME={}", epoch.as_secs()),
                format!("HTTP_METHOD={}", request_method.as_str()),
                format!("HTTP_PATH={}", request_path),
                format!("REMOTE_ADDR={}", connect_info.ip()),
                format!("REMOTE_PORT={}", connect_info.port()),
            ];
            if let Some(query) = path_and_query.query() {
                ultra_thin_info.push(format!("HTTP_QUERY={}", query));
            }

            // Inject all into the passed through values
            let ultra_thin_headers: Vec<String> = upstream_headers
                .iter()
                .filter(ultra_thin_header_filter)
                .map(|(h, v)| {
                    format!(
                        "HTTP_{}={}",
                        h.as_str().to_uppercase().replace("-", "_"),
                        urlencoding::encode(v.to_str().unwrap())
                    )
                })
                .collect();
            ultra_thin_info.extend_from_slice(&ultra_thin_headers);

            if is_ultra_thin(request_path) && request_method == Method::GET {
                // Explicit GET request to /ultra
                upstream_uri = format!("{}&{}", upstream_uri, ultra_thin_info.join("&"));
                reqwest::Body::wrap_stream(request.into_body().into_data_stream())
            } else if is_ultra_thin(request_path)
                && request_method == Method::POST
                && content_type == "application/x-www-form-urlencoded"
            {
                // Explicit POST request to /ultra
                // Remove content length header, so we can modify the POST body (reqwest will figure out the new size)
                upstream_headers.remove(CONTENT_LENGTH);
                build_omnis_body(request.into_body(), &ultra_thin_info).await?
            } else if use_fallback {
                // Fallback to ultra-thin using

                // Remove content length header, so we can modify the POST body (reqwest will figure out the new size)
                upstream_headers.remove(CONTENT_LENGTH);

                // Force upstream method to be a POST and that is form encoded
                upstream_method = Method::POST;
                upstream_headers.insert(CONTENT_TYPE, "application/x-www-form-urlencoded".parse()?);

                let uri: Uri = upstream_uri.parse()?;
                upstream_uri = format!(
                    "{}://{}{}/ultra",
                    uri.scheme().unwrap(),
                    uri.host().unwrap(),
                    match uri.port() {
                        Some(p) => format!(":{}", p),
                        None => String::new(),
                    },
                );

                // Add additional info for the ultra-thin server (must be first in payload)
                ultra_thin_info.insert(
                    0,
                    format!(
                        "OmnisLibrary={}",
                        config.fallback_ultra_thin_library.clone().unwrap()
                    ),
                );
                ultra_thin_info.insert(
                    1,
                    format!(
                        "OmnisClass={}",
                        config.fallback_ultra_thin_class.clone().unwrap()
                    ),
                );

                // Extract body content into bytes
                if request_method != Method::GET {
                    let body_bytes: Vec<u8> = axum::body::to_bytes(request.into_body(), usize::MAX)
                        .await?
                        .to_vec();

                    // Encode as base64 for processing by Ultra-Thin
                    if !body_bytes.is_empty() {
                        ultra_thin_info.push(format!("HTTP_BODY={}", STANDARD.encode(body_bytes)));
                    }
                }

                build_omnis_body(axum::body::Body::from(""), &ultra_thin_info).await?
            } else {
                reqwest::Body::wrap_stream(request.into_body().into_data_stream())
            }
        } else {
            upstream_headers = upstream_headers
                .iter()
                .filter(upstream_header_filter)
                .map(|(h, v)| (h.to_owned(), v.to_owned()))
                .collect();

            reqwest::Body::wrap_stream(request.into_body().into_data_stream())
        };

    let ret = (
        upstream_method,
        upstream_uri,
        upstream_headers,
        upstream_body,
    );
    Ok(ret)
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
    fn new(method: &Method, path: &str, ultra_thin_fallback: bool) -> ConnectionType {
        if method == Method::GET && is_static_asset(path) {
            // Static assets get a fast-path, since they will be cached by this server
            ConnectionType::CacheLoad
        } else if is_javascript_client(path) {
            // JS Client gets a special path for sticky session handling
            ConnectionType::StickySession
        } else if is_rest_api(path) {
            // REST APIs always start with /api
            ConnectionType::Regular(WaitingRoom::Skip)
        } else if is_ultra_thin(path) || ultra_thin_fallback {
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

/// Create a reqwest body that is compatible with Omnis Studio ultra-thin client
async fn build_omnis_body(
    body: axum::body::Body,
    ultra_thin_info: &[String],
) -> Result<reqwest::Body> {
    // Read the body into a local buffer
    let mut bytes: Vec<u8> = axum::body::to_bytes(body, usize::MAX).await?.to_vec();

    // Extend the body with the modified headers
    if !bytes.is_empty() {
        bytes.extend_from_slice("&".as_bytes());
    }
    bytes.extend_from_slice(ultra_thin_info.join("&").as_bytes());

    Ok(reqwest::Body::from(bytes))
}
