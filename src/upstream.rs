use std::collections::HashSet;
use std::ops::{Deref, DerefMut};
use tokio::sync::{RwLock, RwLockReadGuard, RwLockWriteGuard};

/// Single upstream server
#[derive(Clone)]
pub struct Upstream {
    pub id: usize,
    pub uri: String,
    pub removed: bool,
}

impl Upstream {
    fn new(id: usize, uri: impl Into<String>) -> Self {
        Self {
            id,
            uri: uri.into(),
            removed: false,
        }
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
    pub async fn first_uri(&self) -> Option<String> {
        let guard = self._read_lock().await;
        let upstreams = guard.deref();
        upstreams.first_uri()
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
    pub async fn add_upstreams(&self, uris: &[String]) {
        let mut guard = self._write_lock().await;
        let upstreams = guard.deref_mut();
        upstreams.add_uris(uris);
    }

    /// Remove a vector of upstream URIs from the pool
    pub async fn remove_upstreams(&self, uris: &[String]) {
        let mut guard = self._write_lock().await;
        let upstreams = guard.deref_mut();
        upstreams.remove_uris(uris);
    }
}

// Internal pool structure with no locking
struct Pool {
    pool: Vec<Upstream>,
    next_id: usize,
}

impl Pool {
    /// Create a new pool of upstream servers
    fn new() -> Self {
        Self {
            pool: Vec::new(),
            next_id: 1,
        }
    }

    /// vector of all current IDs and URIs in the pool
    fn first_uri(&self) -> Option<String> {
        self.pool.iter().find(|u| !u.removed).map(|u| u.uri.clone())
    }

    /// vector of all current IDs and URIs in the pool
    fn current_uris(&self) -> Vec<(usize, String)> {
        self.pool
            .iter()
            .filter(|u| !u.removed)
            .map(|u| (u.id, u.uri.clone()))
            .collect()
    }

    /// Add 1+ URIs to the upstream pool
    fn add_uris(&mut self, uris: &[String]) {
        // Create unique set of URIs for comparison
        let uri_set: HashSet<String> = self.pool.iter().map(|s| s.uri.clone()).collect();

        // Push new upstream instance
        for uri in uris {
            if uri_set.contains(uri) {
                continue;
            }
            self.pool.push(Upstream::new(self.next_id, uri));
            self.next_id += 1;
        }
    }

    /// Remove 1+ of URIs from the service
    fn remove_uris(&mut self, uris: &[String]) {
        // Create unique set of URIs for comparison
        let uri_set: HashSet<String> = uris.iter().cloned().collect();

        // Strip all matching URIs from the set
        self.pool.retain(|server| !uri_set.contains(&server.uri));
    }
}
