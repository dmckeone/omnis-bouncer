use axum::http::header::CONTENT_TYPE;
use axum::http::HeaderName;
use axum::response::Response;
use axum::{extract, middleware};
use tower_cookies::{Cookie, Cookies};
use tracing::info;

use crate::errors;
use crate::state::AppState;

/// Inject the state into middleware extensions so it can be used in response handlers and
/// other middleware
pub async fn middleware(
    extract::State(state): extract::State<AppState>,
    cookies: Cookies,
    req: extract::Request,
    next: middleware::Next,
) -> errors::Result<Response> {
    let cookie_name = state.config.cookie_name.clone();

    let cookie = cookies.get(&cookie_name);
    let method = req.method().clone();
    let uri = req.uri().clone();

    // Test retrieving something from redis
    // let value = get_key(&state.redis).await?;
    // info!("Received from Redis: {}", value);

    // Extract queue token from cookie
    let queue_token = match cookie.clone() {
        Some(c) => String::from(c.value()),
        None => String::from(state.queue.new_id()),
    };

    // Process request
    let mut resp = next.run(req).await;

    // Attach return cookie
    if cookie.is_none() {
        let mut cookie = Cookie::new(cookie_name, queue_token.clone());
        cookie.set_http_only(true);
        cookie.set_path("/");
        cookies.add(cookie);
    }

    // Extract content type -- maybe don't add header for certain types?
    let content_type = match resp.headers().get(CONTENT_TYPE) {
        Some(v) => String::from(v.to_str()?),
        None => String::from("<unknown>"),
    };

    // Attach matching header (if REST API)
    let header_name = state.config.header_name.clone();
    resp.headers_mut().insert(
        HeaderName::from_lowercase(header_name.as_bytes())?,
        queue_token.clone().parse()?,
    );

    info!("{} {} -> {} {}", method, uri, resp.status(), content_type);
    Ok(resp)
}
