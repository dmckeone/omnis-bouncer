use std::collections::HashSet;
use std::ops::{Deref, DerefMut};
use std::task::Waker;
use tokio::sync::{RwLock, RwLockReadGuard, RwLockWriteGuard};
use tracing::error;

/// Single upstream server
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

    pub fn mark_available(&mut self) {
        self.available = true;
    }
}

// Locked pool of upstream servers (controls locking for public usage)
pub struct UpstreamPool {
    pool: RwLock<Pool>,
}

impl UpstreamPool {
    /// Create a new pool of upstream servers
    pub fn new() -> Self {
        Self {
            pool: RwLock::new(Pool::new()),
        }
    }

    // Utility for generic read lock on the pool
    async fn _read_lock(&self) -> RwLockReadGuard<'_, Pool> {
        self.pool.read().await
    }

    /// Return a vector of tuples with the ID and URI of all active pool URIs
    pub async fn current_uris(&self) -> Vec<(usize, String)> {
        let guard = self._read_lock().await;
        let upstreams = guard.deref();
        upstreams.current_uris()
    }

    // Utility for generic write lock on the pool
    async fn _write_lock(&self) -> RwLockWriteGuard<'_, Pool> {
        self.pool.write().await
    }

    /// Add a vector of upstream URIs to the pool
    pub async fn add_upstreams(&self, uris: &Vec<String>) {
        let mut guard = self._write_lock().await;
        let upstreams = guard.deref_mut();
        upstreams.add_uris(uris);
        upstreams.wake();
    }

    /// Remove a vector of upstream URIs from the pool
    pub async fn remove_upstreams(&self, uris: &Vec<String>) {
        let mut guard = self._write_lock().await;
        let upstreams = guard.deref_mut();
        upstreams.remove_uris(uris);
        upstreams.wake();
    }

    /// Get result of the next discovery poll call (and allow optional waker)
    ///
    /// Intended for use with [futures_core polls](https://docs.rs/futures-core/0.3.31/futures_core/stream/trait.Stream.html)
    pub fn discovery_poll(&self, waker: Option<Waker>) -> PoolPoll {
        match self.pool.try_write() {
            Ok(mut guard) => {
                let upstreams = guard.deref_mut();
                // Optionally set waker if available
                if let Some(waker) = waker {
                    upstreams.waker = Some(waker);
                }
                upstreams.next_poll()
            }
            Err(e) => {
                error!("Unable to lock for poll: {}", e);
                PoolPoll::NoChange
            }
        }
    }
}

/// Pool polling status, when using with async streams
pub enum PoolPoll {
    Insert(usize, String),
    Remove(usize),
    NoChange,
}

// Internal pool structure with no locking
struct Pool {
    pool: Vec<Upstream>,
    waker: Option<Waker>,
    next_id: usize,
}

impl Pool {
    /// Create a new pool of upstream servers
    fn new() -> Self {
        Self {
            pool: Vec::new(),
            waker: None,
            next_id: 1,
        }
    }

    /// vector of all current IDs and URIs in the pool
    fn current_uris(&self) -> Vec<(usize, String)> {
        self.pool
            .iter()
            .filter(|u| (*u).available == true && (*u).removed == false)
            .map(|u| (u.id, u.uri.clone()))
            .collect()
    }

    /// index and mutable upstream to return on the next discovery poll
    fn next_poll(&mut self) -> PoolPoll {
        let item = self
            .pool
            .iter_mut()
            .enumerate()
            .filter(|(_, u)| (*u).available == false || (*u).removed == true)
            .next();

        if let Some((idx, upstream)) = item {
            let id = upstream.id;
            if upstream.removed == true {
                self.remove_pool_index(idx);
                return PoolPoll::Remove(id);
            } else if upstream.available == false {
                upstream.mark_available();
                return PoolPoll::Insert(id, upstream.uri.clone());
            }
        }

        PoolPoll::NoChange
    }

    /// Trigger the waker, if a waker has been assigned
    fn wake(&self) {
        if let Some(w) = self.waker.clone() {
            w.wake();
        }
    }

    fn remove_pool_index(&mut self, index: usize) {
        self.pool.remove(index);
    }

    /// Add 1+ URIs to the upstream pool
    fn add_uris(&mut self, uris: &Vec<String>) {
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

    /// Remove 1+ of URIs from the service
    fn remove_uris(&mut self, uris: &Vec<String>) {
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
