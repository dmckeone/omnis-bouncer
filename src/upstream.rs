use std::collections::HashSet;
use std::ops::{Deref, DerefMut};
use std::sync::Arc;
use tokio::sync::{OwnedSemaphorePermit, RwLock, RwLockReadGuard, RwLockWriteGuard, Semaphore};

/// Upstream specification
#[derive(Clone)]
pub struct Upstream {
    pub uri: String,
    pub clients: usize,
}

impl Upstream {
    pub fn new(uri: impl Into<String>, clients: Option<usize>) -> Self {
        Self {
            uri: uri.into(),
            clients: clients.unwrap_or(1),
        }
    }
}

/// Single backend server
struct UpstreamInner {
    id: usize,
    max_clients: usize,
    sempahore: Arc<Semaphore>,
    uri: String,
    removed: bool,
}

impl UpstreamInner {
    fn new(id: usize, upstream: Upstream) -> Self {
        Self {
            id,
            max_clients: upstream.clients,
            sempahore: Arc::new(Semaphore::new(upstream.clients)),
            uri: upstream.uri,
            removed: false,
        }
    }

    fn current_clients(&self) -> usize {
        self.max_clients - self.sempahore.available_permits()
    }

    fn full(&self) -> bool {
        self.sempahore.available_permits() == 0
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

    /// Return the next available URI in the pool, along with the permit to use it
    pub async fn least_busy_uri(&self) -> Option<String> {
        let guard = self._read_lock().await;
        let upstreams = guard.deref();
        upstreams.least_busy_uri()
    }

    /// Return the next available URI in the pool, along with the permit to use it
    pub async fn acquire_next_uri(&self) -> Option<(OwnedSemaphorePermit, String)> {
        let guard = self._read_lock().await;
        let upstreams = guard.deref();
        upstreams.acquire_next_uri()
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
    pub async fn add_upstreams(&self, uris: &[Upstream]) {
        let mut guard = self._write_lock().await;
        let upstreams = guard.deref_mut();
        upstreams.add_upstreams(uris);
    }

    /// Remove a vector of URIs from the pool
    pub async fn remove_uris(&self, uris: &[String]) {
        let mut guard = self._write_lock().await;
        let upstreams = guard.deref_mut();
        upstreams.remove_uris(uris);
    }
}

// Internal pool structure with no locking
struct Pool {
    pool: Vec<UpstreamInner>,
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

    // Get the URI with least busy sempahore (useful for asset loading where the lock is less critical)
    fn least_busy_uri(&self) -> Option<String> {
        let mut upstreams: Vec<_> = self.pool.iter().filter(|u| !u.removed).collect();
        if upstreams.len() == 0 {
            return None;
        }

        // Sort for least busy upstream server
        upstreams.sort_by_cached_key(|u| u.current_clients());

        match upstreams.first() {
            Some(u) => Some(u.uri.clone()),
            None => None,
        }
    }

    /// Locked URI
    fn acquire_next_uri(&self) -> Option<(OwnedSemaphorePermit, String)> {
        let upstreams = self.pool.iter().filter(|u| !u.removed && !u.full());
        for upstream in upstreams {
            let permit = upstream.sempahore.clone().try_acquire_owned();
            if permit.is_ok() {
                return Some((permit.unwrap(), upstream.uri.clone()));
            }
        }

        None
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
    fn add_upstreams(&mut self, upstreams: &[Upstream]) {
        // Create unique set of URIs for comparison
        let uri_set: HashSet<String> = self.pool.iter().map(|s| s.uri.clone()).collect();

        // Push new upstream instance
        for upstream in upstreams {
            if uri_set.contains(&upstream.uri) {
                continue;
            }
            self.pool
                .push(UpstreamInner::new(self.next_id, upstream.clone()));
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
