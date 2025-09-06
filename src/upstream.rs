use std::collections::HashSet;
use std::ops::DerefMut;
use std::task::Waker;
use tokio::sync::RwLockWriteGuard;

use crate::state::AppState;

#[derive(Clone)]
pub struct Upstream {
    pub id: usize,
    pub uri: String,
    pub available: bool,
    pub removed: bool,
}

impl Upstream {
    fn new(id: usize, uri: impl Into<String>) -> Self {
        Self {
            id,
            uri: uri.into(),
            available: false,
            removed: false,
        }
    }
}

pub struct UpstreamPool {
    pub pool: Vec<Upstream>,
    pub waker: Option<Waker>,
    pub next_id: usize,
}

impl UpstreamPool {
    pub fn new() -> Self {
        Self {
            pool: Vec::new(),
            waker: None,
            next_id: 1,
        }
    }

    pub fn wake(&self) {
        if let Some(w) = self.waker.clone() {
            w.wake();
        }
    }

    // Add 1+ URIs to the upstream pool
    pub fn add(&mut self, uris: &Vec<String>) {
        // Create unique set of URIs for comparison
        let uri_set: HashSet<String> = self.pool.iter().map(|s| s.uri.clone()).collect();

        // Push new upstream instance
        for uri in uris {
            if uri_set.contains(uri) {
                continue;
            }
            self.pool.push(Upstream::new(self.next_id, uri));
            self.next_id = self.next_id + 1;
        }
    }

    // Remove a set of URIs from the service
    pub fn remove(&mut self, uris: &Vec<String>) {
        // Create unique set of URIs for comparison
        let uri_set: HashSet<String> = uris.iter().cloned().collect();

        // Mark all servers that have been removed
        for server in self.pool.iter_mut() {
            if uri_set.contains(&server.uri) {
                server.removed = true;
            }
        }
    }
}

async fn _write_lock(state: &AppState) -> RwLockWriteGuard<'_, UpstreamPool> {
    state.upstream_pool.write().await
}

pub async fn add_upstreams(state: &AppState, uris: &Vec<String>) {
    let mut guard = _write_lock(state).await;
    let upstreams = guard.deref_mut();
    upstreams.add(uris);
    upstreams.wake();
}

pub async fn remove_upstreams(state: &AppState, uris: &Vec<String>) {
    let mut guard = _write_lock(state).await;
    let upstreams = guard.deref_mut();
    upstreams.remove(uris);
    upstreams.wake();
}
