use axum::extract::{OriginalUri, Request, State};
use axum::response::IntoResponse;
use axum_extra::extract::{CookieJar, PrivateCookieJar};
use http::header::{
    ACCEPT, ACCEPT_ENCODING, CONNECTION, CONTENT_ENCODING, CONTENT_LENGTH, CONTENT_TYPE,
    PROXY_AUTHENTICATE, PROXY_AUTHORIZATION, TE, TRAILER, TRANSFER_ENCODING, UPGRADE,
    UPGRADE_INSECURE_REQUESTS,
};
use http::{HeaderMap, HeaderName, HeaderValue, Method, StatusCode};
use lazy_static::lazy_static;
use regex::{Regex, RegexBuilder};
use std::collections::HashSet;
use std::time::Duration;
use tower_cookies::cookie::time::OffsetDateTime;
use tower_cookies::cookie::{Expiration, SameSite};
use tower_cookies::Cookie;
use tracing::{debug, error, info};
use uuid::Uuid;

use crate::errors::Result;
use crate::queue::{Position, QueueControl};
use crate::state::AppState;
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
        RegexBuilder::new(r"^/jschtml/(css|fonts|icons|images|scripts|themes)/$")
            .case_insensitive(true)
            .build()
            .unwrap();
    static ref JSCLIENT_RE: Regex = RegexBuilder::new(r"^/(jschtml|jsclient)")
        .case_insensitive(true)
        .build()
        .unwrap();
    static ref HTML_RE: Regex = RegexBuilder::new(r"\.(htm|html)$")
        .case_insensitive(true)
        .build()
        .unwrap();
}

pub async fn reverse_proxy_handler(
    State(state): State<AppState>,
    method: Method,
    cookies: CookieJar,
    private_cookies: PrivateCookieJar,
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

    // Prepare cookies for any writes
    let mut cookies = cookies;
    let mut private_cookies = private_cookies;

    // Extract cookie values
    let id_cookie = private_cookies.get(config.id_cookie_name.clone().as_str());

    // Extract Queue ID
    let queue = &state.queue;
    let queue_id = extract_queue_id(queue, &id_cookie, &headers, &config.id_http_header);

    // Attach cookie queue ID, if it's new
    if id_cookie.is_none() {
        private_cookies = private_cookies.add(make_server_cookie(
            config.id_cookie_name.clone(),
            String::from(queue_id),
            Some(config.cookie_id_expiration), // 1 day ID expiration
        ));
    }

    // Check if waiting room needs to be displayed
    if method == Method::GET && is_html_page(path) {
        let queue_prefix = config.queue_prefix.clone();
        let position = queue
            .id_position(queue_prefix.clone(), queue_id, None)
            .await?;

        if let Position::Queue(p) = position {
            // Determine general queue status
            let status = queue.queue_status(queue_prefix.clone()).await?;
            let position_string = p.to_string();
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

            cookies = cookies.add(make_browser_cookie(
                config.position_cookie_name.clone(),
                position_string,
            ));
            cookies = cookies.add(make_browser_cookie(
                config.queue_size_cookie_name.clone(),
                size_string,
            ));

            return Ok((
                StatusCode::OK,
                cookies,
                private_cookies,
                waiting_headers,
                axum::body::Body::from(waiting_page),
            ));
        }
    }

    // // Strip waiting room cookies
    cookies = cookies.remove(Cookie::from(config.position_cookie_name.clone()));
    cookies = cookies.remove(Cookie::from(config.queue_size_cookie_name.clone()));

    // Get connection permit for communicating with an upstream server
    let connection_permit =
        get_connection(&state.upstream_pool, method.clone(), path, &queue_id).await;

    let upstream_uri = match connection_permit {
        Some(guard) => format!("{}{:?}", guard.uri, path_and_query),
        None => {
            let mut headers = HeaderMap::new();
            headers.insert(
                HeaderName::from_lowercase(config.id_http_header.as_bytes())?,
                String::from(queue_id).parse()?,
            );

            return Ok((
                StatusCode::SERVICE_UNAVAILABLE,
                cookies,
                private_cookies,
                headers,
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
    let mut response_headers: HeaderMap<HeaderValue> = response
        .headers()
        .iter()
        .filter_map(|(k, v)| match UPSTREAM_IGNORE.contains(k) {
            true => None,
            false => Some((k.to_owned(), v.to_owned())),
        })
        .collect();

    // Attach matching header (if REST API)
    response_headers.insert(
        HeaderName::from_lowercase(config.id_http_header.as_bytes())?,
        String::from(queue_id).parse()?,
    );

    // Copy all response headers except the ones in the ignore list
    let response_status = response.status();
    let response_body = axum::body::Body::from_stream(response.bytes_stream());

    Ok((
        response_status,
        cookies,
        private_cookies,
        response_headers,
        response_body,
    ))
}

// Create a cookie for returning to the caller in a way that can be consumed by Javascript in the
// browser
fn make_browser_cookie<'a>(name: impl Into<String>, value: impl Into<String>) -> Cookie<'a> {
    let name = name.into();
    let value = value.into();

    let mut cookie = Cookie::new(name, value);
    cookie.set_same_site(SameSite::Strict);
    cookie.set_secure(true);
    cookie.set_path("/");
    cookie
}

// Create a cookie that is only accessible by the server, and not consumable by the Javascript
fn make_server_cookie<'a>(
    name: impl Into<String>,
    value: impl Into<String>,
    expiry: Option<Duration>,
) -> Cookie<'a> {
    let name = name.into();
    let value = value.into();

    let mut cookie = Cookie::new(name, value);
    cookie.set_same_site(SameSite::Strict);
    cookie.set_http_only(true);
    cookie.set_secure(true);
    cookie.set_path("/");
    if let Some(expiry) = expiry {
        cookie.set_expires(Expiration::from(OffsetDateTime::now_utc() + expiry));
    }
    cookie
}

/// Extract a queue token from a cookie
fn extract_queue_id(
    queue: &QueueControl,
    cookie: &Option<Cookie>,
    headers: &HeaderMap,
    header_key: &String,
) -> Uuid {
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

    // Extract ID from header
    let header_id = match headers.get(header_key) {
        Some(h) => match h.to_str() {
            Ok(s) => match Uuid::parse_str(s) {
                Ok(c) => Some(c),
                Err(e) => {
                    error!(
                        "Failed to parse Queue ID header \"{}\" as UUID \"{}\": {}",
                        header_key, s, e
                    );
                    None
                }
            },
            Err(e) => {
                error!(
                    "Queue ID header \"{}\" was not a valid string: {}",
                    header_key, e
                );
                None
            }
        },
        None => None,
    };

    // Decide on which ID to use, header takes precedence over cookie, and if both are
    // missing, create a new one
    match (cookie_id, header_id) {
        (Some(_), Some(header_id)) => header_id,
        (Some(cookie_id), None) => cookie_id,
        (None, Some(header_id)) => header_id,
        (None, None) => queue.new_id(),
    }
}

fn is_html_page(path: &str) -> bool {
    HTML_RE.is_match(path)
}

fn is_static_asset(path: &str) -> bool {
    FAVICON_RE.is_match(path) || ASSET_RE.is_match(path)
}

fn is_jsclient(path: &str) -> bool {
    JSCLIENT_RE.is_match(path)
}

// Get a connection permit for the request, based on the method and path.
async fn get_connection<'a>(
    pool: &'a UpstreamPool,
    method: Method,
    path: &str,
    queue_token: &Uuid,
) -> Option<ConnectionPermit<'a>> {
    if method == Method::GET && is_static_asset(path) {
        // Static assets get a fast-path, since they will be cached by this server
        pool.acquire_cache_load_permit().await
    } else if is_jsclient(path) {
        // JS Client gets a special path for sticky session handling
        pool.acquire_sticky_session_permit(queue_token).await
    } else {
        // REST and other requests, can be passed on to any upstream server
        pool.acquire_connection_permit().await
    }
}
