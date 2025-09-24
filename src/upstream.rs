use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::{
    sync::{OwnedSemaphorePermit, RwLock, RwLockReadGuard, RwLockWriteGuard, Semaphore},
    task::JoinSet,
    time::sleep,
};
use tracing::error;
use uuid::Uuid;

/// Upstream specification
#[derive(Debug, Clone, PartialEq)]
pub struct Upstream {
    pub uri: String,
    pub connections: usize,
    pub sticky_sessions: usize,
}

impl Upstream {
    pub fn new(uri: impl Into<String>, connections: usize, sticky_sessions: usize) -> Self {
        Self {
            uri: uri.into(),
            connections,
            sticky_sessions,
        }
    }
}

impl From<&UpstreamServer> for Upstream {
    fn from(upstream_server: &UpstreamServer) -> Self {
        Self {
            uri: upstream_server.uri.clone(),
            connections: upstream_server.max_connections,
            sticky_sessions: upstream_server.max_sticky_sessions,
        }
    }
}

/// Guard that contains the locked URI that can be used for a single reverse proxy call,
/// when the guard is dropped, the permit for that URI is dropped along with it.
pub struct ConnectionPermit {
    pub uri: String,
    _permit: Option<OwnedSemaphorePermit>,
}

impl ConnectionPermit {
    fn new(uri: impl Into<String>, permit: Option<OwnedSemaphorePermit>) -> Self {
        Self {
            uri: uri.into(),
            _permit: permit,
        }
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
            max_sticky_sessions: upstream.sticky_sessions,
            sticky_sessions: Arc::new(RwLock::new(HashMap::with_capacity(
                upstream.sticky_sessions,
            ))),
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
    async fn expire_sticky(&self, now: Instant, expiry: Duration) -> HashSet<Uuid> {
        let mut guard = self.sticky_sessions.write().await;
        let extracted: HashMap<Uuid, Instant> = guard
            .extract_if(|_, i| now.duration_since(*i) < expiry)
            .collect();

        extracted.keys().copied().collect()
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
    pub async fn acquire_cache_load_permit(&self) -> Option<ConnectionPermit> {
        // Acquire the URI, holding the read lock for as little as possible
        let result = {
            let guard = self._read_lock().await;
            (*guard).acquire_cache_load_permit()
        };

        // Transform into URIGuard for consumption, or None if no permits were available
        match result {
            Some(uri) => {
                let guard = ConnectionPermit::new(uri, None);
                Some(guard)
            }
            None => None,
        }
    }

    /// Return the next available URI in the pool, along with the permit to use it
    pub async fn acquire_connection_permit(&self, timeout: Duration) -> Option<ConnectionPermit> {
        // Acquire the URI, holding the read lock for as little as possible
        let result = {
            let guard = self._read_lock().await;
            (*guard).acquire_connection_permit(timeout).await
        };

        // Transform into URIGuard for consumption, or None if no permits were available
        match result {
            Some((permit, uri)) => {
                let guard = ConnectionPermit::new(uri, Some(permit));
                Some(guard)
            }
            None => None,
        }
    }

    /// Return the next available sticky URI in the pool, along with the permit to use it
    pub async fn acquire_sticky_session_permit(
        &self,
        id: &Uuid,
        timeout: Duration,
    ) -> Option<ConnectionPermit> {
        // Acquire the URI, holding the read lock for as little as possible
        let result = {
            let guard = self._read_lock().await;
            (*guard).acquire_sticky_permit(id, timeout).await
        };

        // Transform into URIGuard for consumption, or None if no permits were available
        match result {
            Some((permit, uri)) => {
                let guard = ConnectionPermit::new(uri, Some(permit));
                Some(guard)
            }
            None => None,
        }
    }

    pub async fn expire_sticky_sessions(&self) -> HashSet<Uuid> {
        let guard = self._read_lock().await;
        (*guard).expire_sticky(self.sticky_expiry_secs).await
    }

    /// Return a vector of tuples with the ID and URI of all active pool URIs
    pub async fn upstreams(&self) -> Vec<Upstream> {
        let guard = self._read_lock().await;
        (*guard).upstreams()
    }

    // Utility for generic write lock on the pool
    async fn _write_lock(&self) -> RwLockWriteGuard<'_, Pool> {
        self.pool.write().await
    }

    /// Add a vector of upstream URIs to the pool
    pub async fn add_upstreams(&self, uris: &[Upstream]) {
        let mut guard = self._write_lock().await;
        (*guard).add_upstreams(uris);
    }

    /// Remove a vector of URIs from the pool
    pub async fn remove_uris(&self, uris: &[String]) {
        let mut guard = self._write_lock().await;
        (*guard).remove_uris(uris);
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
            .find(Self::cache_load_filter);

        upstream.map(|u| u.uri.clone())
    }

    /// Acquire a connection URI
    async fn acquire_connection_permit(
        &self,
        timeout: Duration,
    ) -> Option<(OwnedSemaphorePermit, String)> {
        let least_connections = self
            .least_connections()
            .into_iter()
            .filter(Self::acquire_filter);

        for upstream in least_connections {
            if let Ok(permit) = upstream.connection_permits.clone().try_acquire_owned() {
                return Some((permit, upstream.uri.clone()));
            }
        }

        // Unable to quickly find a permit.  Create a join set and wait for the next available
        // semaphore to complete
        let mut set = JoinSet::new();

        // Add timeout value
        set.spawn(async move {
            sleep(timeout).await;
            None
        });

        // Add all upstreams
        for upstream in self.pool.iter().filter(|u| !u.removed) {
            let permits = upstream.connection_permits.clone();
            let uri = upstream.uri.clone();
            set.spawn(async move {
                match permits.acquire_owned().await {
                    Ok(permit) => Some((permit, uri)),
                    Err(e) => {
                        error!("Connection permit error: {}", e);
                        None
                    }
                }
            });
        }

        match set.join_next().await {
            Some(Ok(permit_pair)) => permit_pair,
            _ => None,
        }
    }

    /// Acquire a sticky session URI
    async fn acquire_sticky_permit(
        &self,
        id: &Uuid,
        timeout: Duration,
    ) -> Option<(OwnedSemaphorePermit, String)> {
        for upstream in self.pool.iter() {
            if upstream.contains_id(id).await {
                return Self::existing_sticky_uri(upstream);
            }
        }
        self.new_sticky_uri(id, timeout).await
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

    // Acquire a connection URI
    async fn acquire_sticky_connection_permit(&self) -> Option<(usize, OwnedSemaphorePermit)> {
        let least_sessions = self
            .least_sticky_sessions()
            .await
            .into_iter()
            .filter(Self::acquire_filter);

        for upstream in least_sessions {
            if let Ok(permit) = upstream.connection_permits.clone().try_acquire_owned() {
                return Some((upstream.id, permit));
            }
        }

        None
    }

    /// Attempt to find a new sticky URI by looping for a specified time period until a session is
    /// found or time runs out
    async fn new_sticky_uri(
        &self,
        id: &Uuid,
        timeout: Duration,
    ) -> Option<(OwnedSemaphorePermit, String)> {
        let start = Instant::now();
        loop {
            // Try to acquire a connection and a stick session together
            if let Some((upstream_id, permit)) = self.acquire_sticky_connection_permit().await
                && let Some(upstream) = self.pool.iter().find(|u| u.id == upstream_id)
                && let Ok(()) = upstream.try_add_sticky(id).await
            {
                // Found both a connection and a sticky session.
                return Some((permit, upstream.uri.clone()));
            }

            // Couldn't find any sessions.  Check if timeout expired
            let current = Instant::now();
            if current.duration_since(start) >= timeout {
                return None;
            }

            // Not timed out, wait a second (to save CPU) and try again
            sleep(Duration::from_secs(1)).await;
        }
    }

    /// Remove all sticky sessions that have retired
    async fn expire_sticky(&self, expiry: Duration) -> HashSet<Uuid> {
        let now = Instant::now();
        let mut removed: HashSet<Uuid> = HashSet::new();
        for upstream in self.pool.iter() {
            let upstream_removed = upstream.expire_sticky(now, expiry).await;
            removed = removed.union(&upstream_removed).copied().collect();
        }
        removed
    }

    /// vector of all current IDs and URIs in the pool
    fn upstreams(&self) -> Vec<Upstream> {
        self.pool
            .iter()
            .filter(|u| !u.removed)
            .map(Upstream::from)
            .collect()
    }

    /// Add 1+ URIs to the upstream pool
    fn add_upstreams(&mut self, upstreams: &[Upstream]) {
        // Create unique set of URIs for comparison
        let uri_set: HashSet<String> = self.pool.iter().map(|s| s.uri.clone()).collect();

        // Limit to only URIs that don't exist in the pool already
        let new_upstreams = upstreams.iter().filter(|u| !uri_set.contains(&u.uri));

        // Add new upstream instances to the pool
        for upstream in new_upstreams {
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

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_create_pool() {
        Pool::new();
    }
}
