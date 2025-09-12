use axum::extract::{Request, State};
use axum::response::IntoResponse;
use http::header::{
    CONNECTION, CONTENT_ENCODING, CONTENT_LENGTH, CONTENT_TYPE, PROXY_AUTHENTICATE,
    PROXY_AUTHORIZATION, TE, TRAILER, TRANSFER_ENCODING, UPGRADE,
};
use http::{HeaderMap, HeaderName, HeaderValue, StatusCode};
use lazy_static::lazy_static;
use reqwest;
use std::collections::HashSet;
use std::sync::Arc;
use tower_cookies::{Cookie, Cookies};

use crate::errors::Result;
use crate::state::AppState;

lazy_static! {
    static ref IGNORE: HashSet<HeaderName> = {
        let mut set = HashSet::new();
        set.insert(CONTENT_LENGTH);
        set.insert(CONTENT_ENCODING);
        set.insert(CONNECTION);
        set.insert(PROXY_AUTHENTICATE);
        set.insert(PROXY_AUTHORIZATION);
        set.insert(TE);
        set.insert(TRAILER);
        set.insert(TRANSFER_ENCODING);
        set.insert(UPGRADE);

        set
    };
}

pub async fn reverse_proxy_handler(
    State(state): State<Arc<AppState>>,
    cookies: Cookies,
    request: Request,
) -> Result<impl IntoResponse> {
    // Extract config
    let state = state.clone();
    let config = &state.config;
    let cookie_name = config.cookie_name.clone();

    // Clone properties of the request that are used
    let method = request.method().clone();
    let uri = request.uri().clone();
    let headers = request.headers().clone();
    let body_stream = request.into_body().into_data_stream();

    // Extract cookies from the request
    let cookie = cookies.get(&cookie_name);

    // Extract queue token from cookie
    let queue_token = match cookie.clone() {
        Some(c) => String::from(c.value()),
        None => String::from(state.queue.new_id()),
    };

    // Determine Proxy URI
    let Some(upstream) = state.upstream_pool.first_uri().await else {
        return Ok((
            StatusCode::SERVICE_UNAVAILABLE,
            HeaderMap::new(),
            axum::body::Body::from("Service Unavailable"),
        ));
    };
    let upstream_uri = format!("{}{}", upstream, uri);

    // Process Request on Upstream
    let client = &state.client;
    let response = client
        .request(method, upstream_uri)
        .headers(headers.clone())
        .body(reqwest::Body::wrap_stream(body_stream))
        .send()
        .await?;

    // Attach return cookie
    if cookie.is_none() {
        let mut cookie = Cookie::new(cookie_name, queue_token.clone());
        cookie.set_http_only(true);
        cookie.set_path("/");
        cookies.add(cookie);
    }

    // Extract content type -- maybe don't add header for certain types?
    let content_type = match response.headers().get(CONTENT_TYPE) {
        Some(v) => String::from(v.to_str()?),
        None => String::from("<unknown>"),
    };

    // Build Headers
    let mut response_headers: HeaderMap<HeaderValue> = response
        .headers()
        .iter()
        .filter_map(|(k, v)| match IGNORE.contains(k) {
            true => None,
            false => Some((k.to_owned(), v.to_owned())),
        })
        .collect();

    // Attach matching header (if REST API)
    let header_name = config.header_name.clone();
    response_headers.insert(
        HeaderName::from_lowercase(header_name.as_bytes())?,
        queue_token.clone().parse()?,
    );

    // Copy all response headers except the ones in the ignore list
    let response_status = response.status();
    let response_body = axum::body::Body::from_stream(response.bytes_stream());

    Ok((response_status, response_headers, response_body))
}
