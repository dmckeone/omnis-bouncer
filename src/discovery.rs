use futures_util::Stream;
use std::ops::DerefMut;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use tokio::sync::RwLock;
use tower::discover::Change;

use crate::upstream::UpstreamPool;

/// A simple discovery stream that simulates services being added
#[derive(Clone)]
pub struct SimpleDiscoveryStream {
    pub upstream_pool: Arc<RwLock<UpstreamPool>>,
}

impl SimpleDiscoveryStream {
    pub fn new(upstreams: Arc<RwLock<UpstreamPool>>) -> Self {
        Self {
            upstream_pool: upstreams,
        }
    }
}

impl Stream for SimpleDiscoveryStream {
    type Item = Result<Change<usize, String>, Box<dyn std::error::Error + Send + Sync>>;

    fn poll_next(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut guard = match self.upstream_pool.try_write() {
            Ok(guard) => guard,
            Err(_) => return Poll::Pending,
        };
        let upstreams = guard.deref_mut();

        // Ensure that latest waker is set
        upstreams.waker = Some(_cx.waker().clone());

        let item = upstreams
            .pool
            .iter_mut()
            .enumerate()
            .filter(|(_, u)| (*u).available == false || (*u).removed == true)
            .next();

        let mut status = Poll::Pending;
        if let Some((idx, upstream)) = item {
            if upstream.removed == true {
                let change = Change::Remove(upstream.id);
                upstreams.pool.remove(idx);
                status = Poll::Ready(Some(Ok(change)));
            } else if upstream.available == false {
                let change = Change::Insert(upstream.id, upstream.uri.clone());
                upstream.available = true;
                status = Poll::Ready(Some(Ok(change)));
            }
        }

        status
    }
}
