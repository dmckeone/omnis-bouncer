use axum::extract::{OriginalUri, Request, State};
use axum::response::IntoResponse;
use http::header::{
    ACCEPT, ACCEPT_ENCODING, CONNECTION, CONTENT_ENCODING, CONTENT_LENGTH, CONTENT_TYPE,
    PROXY_AUTHENTICATE, PROXY_AUTHORIZATION, TE, TRAILER, TRANSFER_ENCODING, UPGRADE,
    UPGRADE_INSECURE_REQUESTS,
};
use http::{HeaderMap, HeaderName, HeaderValue, Method, StatusCode};
use lazy_static::lazy_static;
use regex::{Regex, RegexBuilder};
use std::collections::HashSet;
use std::sync::Arc;
use tower_cookies::{Cookie, Cookies};
use tracing::{debug, error, info};
use uuid::Uuid;

use crate::errors::Result;
use crate::state::AppState;

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
    static ref JSCLIENT_RE: Regex = RegexBuilder::new(r"^/jschtml")
        .case_insensitive(true)
        .build()
        .unwrap();
}

fn is_static_asset(path: &str) -> bool {
    FAVICON_RE.is_match(path) || ASSET_RE.is_match(path)
}

fn is_jsclient(path: &str) -> bool {
    JSCLIENT_RE.is_match(path)
}

pub async fn reverse_proxy_handler(
    State(state): State<Arc<AppState>>,
    method: Method,
    cookies: Cookies,
    headers: HeaderMap,
    uri: OriginalUri,
    request: Request,
) -> Result<impl IntoResponse> {
    // Extract config
    let state = state.clone();
    let config = &state.config;
    let cookie_name = config.cookie_name.clone();

    // Clone properties of the request that are used
    let path_and_query = uri.path_and_query().unwrap();
    let path = path_and_query.path();
    let body_stream = request.into_body().into_data_stream();

    // Extract cookies from the request
    let cookie = cookies.get(&cookie_name);

    // Extract queue token from cookie
    let queue_token = match cookie.clone() {
        Some(c) => match Uuid::parse_str(c.value()) {
            Ok(u) => u,
            Err(e) => {
                error!(
                    "Using new ID after cookie parse failure - \"{}\": {}",
                    c.value(),
                    e
                );
                state.queue.new_id()
            }
        },
        None => state.queue.new_id(),
    };

    // Determine Proxy URI, depending on resources needed
    let connection_permit = if method == Method::GET && is_static_asset(path) {
        // Static assets get a fast-path, since they will be cached by this server
        state.upstream_pool.acquire_cache_load_permit().await
    } else if is_jsclient(path) {
        // JS Client gets a special path for sticky session handling
        state
            .upstream_pool
            .acquire_sticky_session_permit(&queue_token)
            .await
    } else {
        // REST and other requests, can be passed on to any upstream server
        state.upstream_pool.acquire_connection_permit().await
    };

    let upstream_uri = match connection_permit {
        Some(guard) => format!("{}{:?}", guard.uri, path_and_query),
        None => {
            // TODO: Waiting room if main JS Client entry point
            error!("No upstream available");
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

    // Attach return cookie
    if cookie.is_none() {
        let mut cookie = Cookie::new(cookie_name, String::from(queue_token));
        cookie.set_http_only(true);
        cookie.set_path("/");
        cookies.add(cookie);
    }

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
    let header_name = config.header_name.clone();
    response_headers.insert(
        HeaderName::from_lowercase(header_name.as_bytes())?,
        String::from(queue_token).parse()?,
    );

    // Copy all response headers except the ones in the ignore list
    let response_status = response.status();
    let response_body = axum::body::Body::from_stream(response.bytes_stream());

    Ok((response_status, response_headers, response_body))
}
