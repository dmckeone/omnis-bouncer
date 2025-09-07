use futures_util::Stream;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use tower::discover::Change;

use crate::upstream::{PoolPoll, UpstreamPool};

/// A simple discovery stream for axum-reverse-proxy that allows the dynamic addition and removal
/// of upstream servers.
#[derive(Clone)]
pub struct UpstreamPoolStream {
    pub upstream_pool: Arc<UpstreamPool>,
}

impl UpstreamPoolStream {
    pub fn new(upstreams: Arc<UpstreamPool>) -> Self {
        Self {
            upstream_pool: upstreams,
        }
    }
}

impl Stream for UpstreamPoolStream {
    type Item = Result<Change<usize, String>, Box<dyn std::error::Error + Send + Sync>>;

    fn poll_next(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match self.upstream_pool.discovery_poll(Some(_cx.waker().clone())) {
            PoolPoll::Insert(idx, uri) => Poll::Ready(Some(Ok(Change::Insert(idx, uri)))),
            PoolPoll::Remove(idx) => Poll::Ready(Some(Ok(Change::Remove(idx)))),
            PoolPoll::NoChange => Poll::Pending,
        }
    }
}
