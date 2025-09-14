use dashmap::DashMap;
use std::collections::{HashMap, HashSet};
use std::ops::{Deref, DerefMut};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{
    Mutex, OwnedSemaphorePermit, RwLock, RwLockReadGuard, RwLockWriteGuard, Semaphore,
};
use tracing::{error, info};
use uuid::Uuid;

/// Upstream specification
#[derive(Clone)]
pub struct Upstream {
    pub uri: String,
    pub connections: usize,
    pub clients: usize,
}

// Guard that contains the locked URI that can be used for a single reverse proxy call,
// when the guard is dropped, the permit for that URI is dropped along with it.
pub struct UriGuard<'a> {
    pub uri: String,
    permit: OwnedSemaphorePermit,
    upstream_pool: &'a UpstreamPool,
}

impl<'a> UriGuard<'a> {
    fn new(
        uri: impl Into<String>,
        permit: OwnedSemaphorePermit,
        upstream_pool: &'a UpstreamPool,
    ) -> Self {
        Self {
            uri: uri.into(),
            permit,
            upstream_pool,
        }
    }
}

// Create guard that alerts the pool when a URI is freed
impl<'a> Drop for UriGuard<'a> {
    fn drop(&mut self) {
        self.upstream_pool.notify_free_uri(self.uri.clone());
    }
}

impl Upstream {
    pub fn new(uri: impl Into<String>, connections: usize, clients: usize) -> Self {
        Self {
            uri: uri.into(),
            connections,
            clients,
        }
    }
}

/// Single backend server
#[derive(Clone)]
struct UpstreamInner {
    id: usize,
    max_connections: usize,
    permits: Arc<Semaphore>,
    max_sticky: usize,
    sticky: Arc<RwLock<HashMap<Uuid, Instant>>>,
    uri: String,
    removed: bool,
}

enum UpstreamStickyError {
    Full,
}

impl UpstreamInner {
    fn new(id: usize, upstream: Upstream) -> Self {
        Self {
            id,
            max_connections: upstream.connections,
            permits: Arc::new(Semaphore::new(upstream.connections)),
            max_sticky: upstream.clients,
            sticky: Arc::new(RwLock::new(HashMap::with_capacity(upstream.clients))),
            uri: upstream.uri,
            removed: false,
        }
    }

    async fn contains_id(&self, id: &Uuid) -> bool {
        let guard = self.sticky.read().await;
        guard.contains_key(id)
    }

    async fn current_sticky(&self) -> usize {
        let guard = self.sticky.read().await;
        guard.len()
    }

    async fn try_add_sticky(&self, id: &Uuid) -> Result<(), UpstreamStickyError> {
        let mut guard = self.sticky.write().await;
        if guard.len() >= self.max_sticky {
            return Err(UpstreamStickyError::Full);
        }
        guard.insert(*id, Instant::now());
        Ok(())
    }

    async fn expire_sticky(&self, now: Instant, expiry: Duration) {
        let mut guard = self.sticky.write().await;
        guard.retain(|_, i| now.duration_since(*i) < expiry);
    }

    async fn full_sticky(&self) -> bool {
        self.current_sticky().await >= self.max_sticky
    }

    fn current_connections(&self) -> usize {
        self.max_connections - self.permits.available_permits()
    }

    fn full(&self) -> bool {
        self.permits.available_permits() == 0
    }
}

// Locked pool of upstream servers (controls locking for public usage)
pub struct UpstreamPool {
    pool: RwLock<Pool>,
    sticky_expiry_secs: Duration,
}

impl UpstreamPool {
    /// Create a new pool of upstream servers
    pub fn new(sticky_expiry_secs: Duration) -> Self {
        Self {
            pool: RwLock::new(Pool::new()),
            sticky_expiry_secs,
        }
    }

    // Utility for generic read lock on the pool
    async fn _read_lock(&self) -> RwLockReadGuard<'_, Pool> {
        self.pool.read().await
    }

    /// Return the next available URI in the pool, along with the permit to use it
    pub async fn acquire_uri(&self) -> Option<UriGuard<'_>> {
        // Acquire the URI, holding the read lock for as little as possible
        let result = {
            let guard = self._read_lock().await;
            let upstreams = guard.deref();
            upstreams.acquire_uri().await
        };

        // Transform into URIGuard for consumption, or None if no permits were available
        match result {
            Some((permit, uri)) => {
                let guard = UriGuard::new(uri, permit, self);
                Some(guard)
            }
            None => None,
        }
    }

    /// Return the next available sticky URI in the pool, along with the permit to use it
    pub async fn acquire_sticky_uri(&self, id: &Uuid) -> Option<UriGuard<'_>> {
        // Acquire the URI, holding the read lock for as little as possible
        let result = {
            let guard = self._read_lock().await;
            let upstreams = guard.deref();
            upstreams.expire_sticky(self.sticky_expiry_secs).await;
            upstreams.acquire_sticky_uri(id).await
        };

        // Transform into URIGuard for consumption, or None if no permits were available
        match result {
            Some((permit, uri)) => {
                let guard = UriGuard::new(uri, permit, self);
                Some(guard)
            }
            None => None,
        }
    }

    fn notify_free_uri(&self, uri: String) {
        // TODO: Perhaps do something with dropped URIs
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

    fn acquire_filter() -> fn(&&UpstreamInner) -> bool {
        |u| !u.removed && !u.full()
    }

    async fn acquire_uri(&self) -> Option<(OwnedSemaphorePermit, String)> {
        for upstream in self.pool.iter().filter(Self::acquire_filter()) {
            if let Ok(permit) = upstream.permits.clone().try_acquire_owned() {
                return Some((permit, upstream.uri.clone()));
            }
        }
        None
    }

    async fn acquire_sticky_uri(&self, id: &Uuid) -> Option<(OwnedSemaphorePermit, String)> {
        for upstream in self.pool.iter() {
            if upstream.contains_id(id).await {
                info!("Re-use sticky UUID: {}", id);
                return Self::existing_sticky_uri(upstream);
            }
        }

        self.new_sticky_uri(id).await
    }

    fn existing_sticky_uri(upstream: &UpstreamInner) -> Option<(OwnedSemaphorePermit, String)> {
        // ID already exists in a given upstream, just return the URI if it's not full
        match upstream.permits.clone().try_acquire_owned() {
            Ok(permit) => Some((permit, upstream.uri.clone())),
            Err(error) => {
                error!("Failed to acquire sticky permit: {}", error);
                None
            }
        }
    }

    async fn new_sticky_uri(&self, id: &Uuid) -> Option<(OwnedSemaphorePermit, String)> {
        for upstream in self.pool.iter().filter(Self::acquire_filter()) {
            if let Ok(permit) = upstream.permits.clone().try_acquire_owned() {
                if let Ok(()) = upstream.try_add_sticky(id).await {
                    info!("Add sticky UUID: {}", id);
                    return Some((permit, upstream.uri.clone()));
                }
            }
        }
        info!("Unable to locate to sticky UUID slot: {}", id);
        None
    }

    /// Remove all sticky sessions that have retired
    async fn expire_sticky(&self, expiry: Duration) {
        let now = Instant::now();
        for u in self.pool.iter() {
            u.expire_sticky(now, expiry).await;
        }
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
