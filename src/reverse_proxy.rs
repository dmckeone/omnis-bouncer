use axum::extract::{OriginalUri, Request, State};
use axum::response::IntoResponse;
use http::header::{
    ACCEPT, ACCEPT_ENCODING, CONNECTION, CONTENT_ENCODING, CONTENT_LENGTH, CONTENT_TYPE,
    PROXY_AUTHENTICATE, PROXY_AUTHORIZATION, TE, TRAILER, TRANSFER_ENCODING, UPGRADE,
    UPGRADE_INSECURE_REQUESTS,
};
use http::{HeaderMap, HeaderName, HeaderValue, Method, StatusCode};
use http_types::Mime;
use lazy_static::lazy_static;
use regex::{Regex, RegexBuilder};
use std::collections::HashSet;
use std::str::FromStr;
use std::time::Duration;
use tower_cookies::cookie::time::OffsetDateTime;
use tower_cookies::cookie::{Expiration, SameSite};
use tower_cookies::{Cookie, Cookies, PrivateCookies};
use tracing::{error, info};
use uuid::Uuid;

use crate::errors::Result;
use crate::queue::{QueueControl, QueuePosition};
use crate::state::{AppState, Config};
use crate::upstream::{ConnectionPermit, UpstreamPool};

lazy_static! {
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

pub async fn omnis_studio_reverse_proxy(
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
    let body_stream = request.into_body().into_data_stream();

    // Private Cookies
    let private_cookies = cookies.private(&config.cookie_secret_key);

    let connection_type = ConnectionType::new(&method, &headers, path);
    if connection_type == ConnectionType::Reject {
        return Ok((
            StatusCode::NOT_FOUND,
            HeaderMap::new(),
            axum::body::Body::from("Not Found"),
        ));
    }

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
            return Ok((StatusCode::OK, waiting_headers, waiting_body));
        }

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

    // Process connection permit
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

    // Process Request on Upstream
    let client = &state.client;
    let response = client
        .request(method.clone(), upstream_uri.clone())
        .headers(headers.clone())
        .body(reqwest::Body::wrap_stream(body_stream))
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

async fn check_waiting_page(
    config: &Config,
    cookies: &Cookies,
    queue: &QueueControl,
    queue_id: QueueId,
) -> Result<Option<(HeaderMap, axum::body::Body)>> {
    let queue_prefix = config.queue_prefix.clone();

    let position = match queue_id {
        QueueId::New(id) => queue.id_add(queue_prefix.clone(), id.into(), None).await?,
        QueueId::Existing(id) => {
            queue
                .id_position(queue_prefix.clone(), id.into(), None)
                .await?
        }
    };

    let position = match position {
        QueuePosition::Queue(pos) => pos,
        QueuePosition::Store => return Ok(None),
    };

    // Determine general queue status
    let status = queue.queue_status(queue_prefix.clone()).await?;
    let position_string = position.to_string();
    let size_string = status.queue_size.to_string();

    // Fetch waiting page
    let waiting_page = queue
        .waiting_page(queue_prefix.clone())
        .await?
        .unwrap_or(format!(
            "Waiting Page - {} of {}",
            position_string, size_string
        ));

    let waiting_headers = {
        let mut waiting_headers = HeaderMap::new();
        waiting_headers.insert(
            HeaderName::from_lowercase(config.position_http_header.as_bytes())?,
            position_string.parse()?,
        );
        waiting_headers.insert(
            HeaderName::from_lowercase(config.queue_size_http_header.as_bytes())?,
            size_string.parse()?,
        );
        waiting_headers
    };

    add_browser_cookie(
        cookies,
        config.position_cookie_name.clone(),
        position_string,
    );
    add_browser_cookie(cookies, config.queue_size_cookie_name.clone(), size_string);

    Ok(Some((
        waiting_headers,
        axum::body::Body::from(waiting_page),
    )))
}

enum CookieStatus {
    Added,
    Unchanged,
}

// Return true if a cookie exists with the given value already set
fn cookie_exists(cookies: &Cookies, name: impl Into<String>, value: impl Into<String>) -> bool {
    let name = name.into();
    let value = value.into();
    match cookies.get(name.as_str()) {
        Some(cookie) => cookie.value() == value,
        None => false,
    }
}

// Create a cookie for returning to the caller in a way that can be consumed by Javascript in the
// browser
fn add_browser_cookie(
    cookies: &Cookies,
    name: impl Into<String>,
    value: impl Into<String>,
) -> CookieStatus {
    let name = name.into();
    let value = value.into();

    if cookie_exists(cookies, name.clone(), value.clone()) {
        return CookieStatus::Unchanged;
    }

    cookies.add(browser_cookie(name, value));
    CookieStatus::Added
}

fn browser_cookie<'a>(name: String, value: String) -> Cookie<'a> {
    Cookie::build((name, value))
        .same_site(SameSite::Strict)
        .secure(true)
        .path("/")
        .build()
}

fn server_cookie<'a>(name: String, value: String, expiry: Option<Duration>) -> Cookie<'a> {
    let mut cookie = Cookie::build((name, value))
        .same_site(SameSite::Strict)
        .secure(true)
        .path("/")
        .build();

    if let Some(expiry) = expiry {
        cookie.set_expires(Expiration::from(OffsetDateTime::now_utc() + expiry));
    }
    cookie
}

// Return true if a cookie exists with the given value already set
fn private_cookie_exists(
    cookies: &PrivateCookies,
    name: impl Into<String>,
    value: impl Into<String>,
) -> bool {
    let name = name.into();
    let value = value.into();
    match cookies.get(name.as_str()) {
        Some(cookie) => cookie.value() == value,
        None => false,
    }
}

// Create a cookie that is only accessible by the server, and not consumable by the Javascript
fn add_private_server_cookie(
    cookies: &PrivateCookies,
    name: impl Into<String>,
    value: impl Into<String>,
    expiry: Option<Duration>,
) -> CookieStatus {
    let name = name.into();
    let value = value.into();
    if private_cookie_exists(cookies, name.clone(), value.clone()) {
        return CookieStatus::Unchanged;
    }

    cookies.add(server_cookie(name, value, expiry));
    CookieStatus::Added
}

#[derive(Copy, Clone, Debug)]
enum QueueId {
    New(Uuid),
    Existing(Uuid),
}

impl From<QueueId> for String {
    fn from(queue_id: QueueId) -> Self {
        match queue_id {
            QueueId::New(id) => String::from(id),
            QueueId::Existing(id) => String::from(id),
        }
    }
}

impl From<QueueId> for Uuid {
    fn from(queue_id: QueueId) -> Self {
        match queue_id {
            QueueId::New(id) => id,
            QueueId::Existing(id) => id,
        }
    }
}

/// Extract a queue token from a cookie
fn extract_queue_id(queue: &QueueControl, cookie: &Option<Cookie>) -> QueueId {
    // Extract ID from cookie
    let cookie_id = match cookie {
        Some(c) => match Uuid::parse_str(c.value()) {
            Ok(c) => Some(c),
            Err(e) => {
                error!(
                    "Failed to parse Queue ID cookie as UUID \"{}\": {}",
                    c.value(),
                    e
                );
                None
            }
        },
        None => None,
    };

    // Return existing ID, or create a new one
    match cookie_id {
        Some(cookie_id) => QueueId::Existing(cookie_id),
        None => QueueId::New(queue.new_id()),
    }
}

fn is_html_page(headers: &HeaderMap, path: &str) -> bool {
    // Parse Accept header to see if the client is requesting HTML
    let requires_html = match headers.get(ACCEPT) {
        Some(v) => match v.to_str() {
            Ok(s) => match Mime::from_str(s) {
                Ok(m) => m.essence() == "text/html",
                Err(_) => false,
            },
            Err(_) => false,
        },
        None => false,
    };

    requires_html || HTML_RE.is_match(path)
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
enum ConnectionType {
    CacheLoad,
    StickySession,
    Regular(WaitingRoom),
    Reject,
}

impl ConnectionType {
    // Get a connection permit for the request, based on the method and path.
    fn new(method: &Method, headers: &HeaderMap, path: &str) -> ConnectionType {
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
            if method == Method::GET && is_html_page(headers, path) {
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

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum WaitingRoom {
    Required,
    Skip,
}

// Get a connection permit for the request, based on the method and path.
async fn get_connection(
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
        ConnectionType::Reject => panic!("Should never be called with Reject"),
    }
}
