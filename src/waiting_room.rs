use axum_extra::extract::cookie::Cookie;
use http::{header::CONTENT_TYPE, HeaderMap, HeaderName};
use tower_cookies::Cookies;
use tracing::error;
use uuid::Uuid;

use crate::config::Config;
use crate::queue::{QueueControl, QueuePosition};
use crate::{cookies, errors};

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum QueueId {
    New(Uuid),
    Existing(Uuid),
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum WaitingRoom {
    Required,
    Skip,
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
pub fn extract_queue_id(queue: &QueueControl, cookie: &Option<Cookie>) -> QueueId {
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

// Build a
pub async fn check_waiting_page(
    config: &Config,
    cookies: &Cookies,
    queue: &QueueControl,
    queue_id: QueueId,
) -> errors::Result<Option<(HeaderMap, axum::body::Body)>> {
    let queue_prefix = config.queue_prefix.clone();

    let position = queue
        .id_position(queue_prefix.clone(), queue_id.into(), None, true)
        .await?;

    let position = match position {
        QueuePosition::NotPresent => unreachable!(),
        QueuePosition::Queue(pos) => pos,
        QueuePosition::Store => return Ok(None),
    };

    // Determine general queue status
    let status = queue.queue_status(queue_prefix.clone()).await?;
    let position_string = position.to_string();
    let size_string = status.queue_size.to_string();

    // Fetch waiting page
    let mut waiting_headers = HeaderMap::new();
    waiting_headers.insert(CONTENT_TYPE, "text/html".parse()?);

    let waiting_page_body: axum::body::Body =
        queue.cached_waiting_page(queue_prefix.clone()).await.into();

    waiting_headers.insert(
        HeaderName::from_lowercase(config.position_http_header.as_bytes())?,
        position_string.parse()?,
    );
    waiting_headers.insert(
        HeaderName::from_lowercase(config.queue_size_http_header.as_bytes())?,
        size_string.parse()?,
    );

    cookies::add_browser_cookie(
        cookies,
        config.position_cookie_name.clone(),
        position_string,
    );
    cookies::add_browser_cookie(cookies, config.queue_size_cookie_name.clone(), size_string);

    Ok(Some((waiting_headers, waiting_page_body)))
}
