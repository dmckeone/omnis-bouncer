use std::collections::{HashMap, HashSet};
use std::ops::{Deref, DerefMut};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{OwnedSemaphorePermit, RwLock, RwLockReadGuard, RwLockWriteGuard, Semaphore};
use tracing::{error, info};
use uuid::Uuid;

/// Upstream specification
#[derive(Clone)]
pub struct Upstream {
    pub uri: String,
    pub connections: usize,
    pub clients: usize,
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

/// Guard that contains the locked URI that can be used for a single reverse proxy call,
/// when the guard is dropped, the permit for that URI is dropped along with it.
pub struct ConnectionPermit<'a> {
    pub uri: String,
    upstream_pool: &'a UpstreamPool,
    _permit: Option<OwnedSemaphorePermit>,
}

impl<'a> ConnectionPermit<'a> {
    fn new(
        uri: impl Into<String>,
        permit: Option<OwnedSemaphorePermit>,
        upstream_pool: &'a UpstreamPool,
    ) -> Self {
        Self {
            uri: uri.into(),
            upstream_pool,
            _permit: permit,
        }
    }
}

// Create guard that alerts the pool when a URI is freed
impl<'a> Drop for ConnectionPermit<'a> {
    fn drop(&mut self) {
        self.upstream_pool.notify_free_uri(self.uri.clone());
    }
}

/// Error type for when a sticky session cannot be created due to the upstream being full
enum UpstreamStickyError {
    Full,
}

/// Inner Upstream container for a single upstream server including connection limits, sticky session handling, and URI information.
struct UpstreamServer {
    id: usize,
    max_connections: usize,
    connection_permits: Arc<Semaphore>,
    max_sticky_sessions: usize,
    sticky_sessions: Arc<RwLock<HashMap<Uuid, Instant>>>,
    uri: String,
    removed: bool,
}

impl UpstreamServer {
    /// Construct a new upstream server
    ///
    /// # Parameters
    /// * `id` - Unique identifier for this upstream instance
    /// * `upstream` - Configuration containing connection and client limits, URI, and other upstream settings
    ///
    /// # Example
    /// ```rust
    /// UpstreamInner::new(1, Upstream::new("http://example.com", 100, 50));
    /// ```
    fn new(id: usize, upstream: Upstream) -> Self {
        Self {
            id,
            max_connections: upstream.connections,
            connection_permits: Arc::new(Semaphore::new(upstream.connections)),
            max_sticky_sessions: upstream.clients,
            sticky_sessions: Arc::new(RwLock::new(HashMap::with_capacity(upstream.clients))),
            uri: upstream.uri,
            removed: false,
        }
    }

    /// Checks if the upstream server is stuck to the given ID
    ///
    /// # Locking
    /// This function acquires an internal read lock to ensure thread-safe
    /// modification of the sticky sessions.
    async fn contains_id(&self, id: &Uuid) -> bool {
        let guard = self.sticky_sessions.read().await;
        guard.contains_key(id)
    }

    /// Returns the current count of IDs stuck to this upstream server
    ///
    /// # Locking
    /// This function acquires an internal read lock to ensure thread-safe
    /// modification of the sticky sessions.
    async fn current_sticky(&self) -> usize {
        let guard = self.sticky_sessions.read().await;
        guard.len()
    }

    /// Attempt to stick an ID to this upstream server, `UpstreamStickyError:Full` if full
    ///
    /// # Locking
    /// This function acquires an internal write lock to ensure thread-safe
    /// modification of the sticky sessions.
    async fn try_add_sticky(&self, id: &Uuid) -> Result<(), UpstreamStickyError> {
        if self.full_sticky().await {
            // EARLY EXIT: Check if the serves is already at capacity before acquiring the write
            //             lock
            return Err(UpstreamStickyError::Full);
        }

        let mut guard = self.sticky_sessions.write().await;
        if guard.len() >= self.max_sticky_sessions {
            return Err(UpstreamStickyError::Full);
        }
        guard.insert(*id, Instant::now());
        Ok(())
    }

    /// Remove all expired sticky IDs from this upstream server
    ///
    /// # Parameters
    /// * `now` - The timestamp to consider as "now"
    /// * `expiry` - The duration that a sticky session should last
    ///
    /// # Locking
    /// This function acquires an internal write lock to ensure thread-safe
    /// modification of the sticky sessions.
    async fn expire_sticky(&self, now: Instant, expiry: Duration) {
        let mut guard = self.sticky_sessions.write().await;
        guard.retain(|_, i| now.duration_since(*i) < expiry);
    }

    /// Check if the upstream server sticky session pool is currently full
    ///
    /// # Locking
    /// This function acquires an internal read lock to ensure thread-safe
    /// modification of the sticky sessions.
    async fn full_sticky(&self) -> bool {
        self.current_sticky().await >= self.max_sticky_sessions
    }

    /// Number of current connections against the upstream server
    fn current_connections(&self) -> usize {
        self.max_connections - self.connection_permits.available_permits()
    }

    /// Check if the upstream connection pool is currently full
    fn full(&self) -> bool {
        self.connection_permits.available_permits() == 0
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

    /// Return the URI in the pool with the least connections for a cache load.  This circumvents
    /// most locks, since follow-up connections will be fully cached
    pub async fn acquire_cache_load_permit(&self) -> Option<ConnectionPermit<'_>> {
        // Acquire the URI, holding the read lock for as little as possible
        let result = {
            let guard = self._read_lock().await;
            let upstreams = guard.deref();
            upstreams.acquire_cache_load_permit()
        };

        // Transform into URIGuard for consumption, or None if no permits were available
        match result {
            Some(uri) => {
                let guard = ConnectionPermit::new(uri, None, self);
                Some(guard)
            }
            None => None,
        }
    }

    /// Return the next available URI in the pool, along with the permit to use it
    pub async fn acquire_connection_permit(&self) -> Option<ConnectionPermit<'_>> {
        // Acquire the URI, holding the read lock for as little as possible
        let result = {
            let guard = self._read_lock().await;
            let upstreams = guard.deref();
            upstreams.acquire_connection_permit().await
        };

        // Transform into URIGuard for consumption, or None if no permits were available
        match result {
            Some((permit, uri)) => {
                let guard = ConnectionPermit::new(uri, Some(permit), self);
                Some(guard)
            }
            None => None,
        }
    }

    /// Return the next available sticky URI in the pool, along with the permit to use it
    pub async fn acquire_sticky_session_permit(&self, id: &Uuid) -> Option<ConnectionPermit<'_>> {
        // Acquire the URI, holding the read lock for as little as possible
        let result = {
            let guard = self._read_lock().await;
            let upstreams = guard.deref();
            match upstreams.acquire_sticky_permit(id).await {
                Some(r) => Some(r),
                None => {
                    // If no permits could be found, expire existing sessions, and see if we can
                    // find one after that
                    upstreams.expire_sticky(self.sticky_expiry_secs).await;
                    upstreams.acquire_sticky_permit(id).await
                }
            }
        };

        // Transform into URIGuard for consumption, or None if no permits were available
        match result {
            Some((permit, uri)) => {
                let guard = ConnectionPermit::new(uri, Some(permit), self);
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
    pool: Vec<UpstreamServer>,
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

    /// Vector of UpstreamServer references sorted by least sticky sessions
    async fn least_sticky_sessions(&self) -> Vec<&UpstreamServer> {
        let mut upstreams: Vec<(usize, usize, &UpstreamServer)> = Vec::new();

        for upstream in self.pool.iter() {
            let current_sticky = upstream.current_sticky().await;
            let current_conns = upstream.current_connections();
            upstreams.push((current_sticky, current_conns, upstream))
        }

        upstreams.sort_by_cached_key(|ls| (ls.0, ls.1));

        upstreams.iter().map(|(_, _, u)| *u).collect()
    }

    /// Vector of UpstreamServer references sorted by least connections
    fn least_connections(&self) -> Vec<&UpstreamServer> {
        let mut upstreams: Vec<(usize, &UpstreamServer)> = self
            .pool
            .iter()
            .map(|u| (u.current_connections(), u))
            .collect();

        upstreams.sort_by_cached_key(|ls| ls.0);

        upstreams.iter().map(|(_, u)| *u).collect()
    }

    fn cache_load_filter(u: &&UpstreamServer) -> bool {
        !u.removed && !u.full()
    }

    fn acquire_filter(u: &&UpstreamServer) -> bool {
        !u.removed && !u.full()
    }

    /// Acquire cache loading URI
    fn acquire_cache_load_permit(&self) -> Option<String> {
        let upstream = self
            .least_connections()
            .into_iter()
            .filter(Self::cache_load_filter)
            .next();

        match upstream {
            Some(u) => Some(u.uri.clone()),
            None => None,
        }
    }

    /// Acquire a connection URI
    async fn acquire_connection_permit(&self) -> Option<(OwnedSemaphorePermit, String)> {
        let least_connections = self
            .least_connections()
            .into_iter()
            .filter(Self::acquire_filter);

        for upstream in least_connections {
            if let Ok(permit) = upstream.connection_permits.clone().try_acquire_owned() {
                return Some((permit, upstream.uri.clone()));
            }
        }
        None
    }

    /// Acquire a sticky session URI
    async fn acquire_sticky_permit(&self, id: &Uuid) -> Option<(OwnedSemaphorePermit, String)> {
        for upstream in self.pool.iter() {
            if upstream.contains_id(id).await {
                return Self::existing_sticky_uri(upstream);
            }
        }
        self.new_sticky_uri(id).await
    }

    fn existing_sticky_uri(upstream: &UpstreamServer) -> Option<(OwnedSemaphorePermit, String)> {
        // ID already exists in a given upstream, just return the URI if it's not full
        match upstream.connection_permits.clone().try_acquire_owned() {
            Ok(permit) => Some((permit, upstream.uri.clone())),
            Err(error) => {
                error!("Failed to acquire sticky permit: {}", error);
                None
            }
        }
    }

    async fn new_sticky_uri(&self, id: &Uuid) -> Option<(OwnedSemaphorePermit, String)> {
        let least_sticky = self
            .least_sticky_sessions()
            .await
            .into_iter()
            .filter(Self::acquire_filter);

        for upstream in least_sticky {
            if let Ok(permit) = upstream.connection_permits.clone().try_acquire_owned() {
                if let Ok(()) = upstream.try_add_sticky(id).await {
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
                .push(UpstreamServer::new(self.next_id, upstream.clone()));
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
