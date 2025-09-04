use futures_util::Stream;
use std::collections::HashSet;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::pin::Pin;
use std::sync::{Arc, Mutex, RwLock};
use std::task::{Context, Poll, Waker};
use tower::discover::Change;
use tracing::{error, info};

use crate::state::AppState;

fn awake_balancer(state: &AppState) {
    {
        let waker_guard = state.upstream_waker.lock().unwrap();
        match waker_guard.as_ref() {
            Some(w) => w.clone().wake(),
            None => {}
        }
    }
}

pub fn add_upstreams(state: AppState, new_upstreams: &Vec<String>) {
    // Add all new upstream servers
    let current_upstreams = state.upstreams.clone();
    {
        let mut upstreams_guard = current_upstreams.write().unwrap();
        for upstream in new_upstreams.iter() {
            if !(*upstreams_guard).contains(upstream) {
                (*upstreams_guard).push(upstream.clone())
            }
        }
    }
    // Record the new state of the upstream values
    {
        let upstreams_guard = current_upstreams.read().unwrap();
        info!("Updated upstream servers: {:?}", *upstreams_guard);
    }

    // Signal the discovery process to wake up and look for new upstream servers
    awake_balancer(&state);
}

pub fn remove_upstreams(state: AppState, remove_upstreams: &Vec<String>) {
    // Push a new server
    let current_upstreams = state.upstreams.clone();
    {
        let mut upstreams_guard = current_upstreams.write().unwrap();
        let old_upstreams = (*upstreams_guard).clone();
        for (i, old_upstream) in old_upstreams.iter().enumerate().rev() {
            if remove_upstreams.contains(old_upstream) {
                (*upstreams_guard).remove(i);
            }
        }
    }
    // Record the new state of the upstream values
    {
        let upstreams_guard = current_upstreams.read().unwrap();
        info!("Updated upstream servers: {:?}", *upstreams_guard);
    }

    awake_balancer(&state);
}

/// A simple discovery stream that simulates services being added
#[derive(Clone)]
pub struct SimpleDiscoveryStream {
    pub servers: Arc<RwLock<Vec<String>>>,
    pub waker: Arc<Mutex<Option<Waker>>>,

    // Server status when adding/removing servers
    _last_known: HashSet<String>,
    _insertion: HashSet<String>,
    _removal: HashSet<String>,
}

impl SimpleDiscoveryStream {
    pub fn new(services: Arc<RwLock<Vec<String>>>, waker: Arc<Mutex<Option<Waker>>>) -> Self {
        Self {
            servers: services,
            waker,

            _last_known: HashSet::new(),
            _insertion: HashSet::new(),
            _removal: HashSet::new(),
        }
    }
}

// Simple hash of string to use as a key
fn hash_string(server: &String) -> usize {
    let mut hasher = DefaultHasher::new();
    server.hash(&mut hasher);
    hasher.finish() as usize
}

impl Stream for SimpleDiscoveryStream {
    type Item = Result<Change<usize, String>, Box<dyn std::error::Error + Send + Sync>>;

    fn poll_next(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        // Store waker
        let waker = self.waker.clone();
        let mut waker_guard = waker.lock().unwrap();
        *waker_guard = Some(_cx.waker().clone());

        // Local clone of services guard
        let servers = self.servers.clone();

        // Acquire read lock on server list
        let servers_guard = match servers.read() {
            Ok(sg) => sg,
            Err(e) => {
                error!("Failed to acquire server lock: {}", e);
                return Poll::Pending;
            }
        };

        let last_set = self._last_known.clone();
        let current_set = servers_guard.iter().cloned().collect::<HashSet<_>>();

        // Set new last known state
        self._last_known = current_set.clone();

        // Update removals and remove any insertions if they were removed
        let removed: Vec<_> = last_set.difference(&current_set).cloned().collect();
        for r in removed.iter() {
            self._insertion.remove(r);
            self._removal.insert(r.clone());
        }

        // Update additions and remove any removal if they were re-added
        let added: Vec<_> = current_set.difference(&last_set).cloned().collect();
        for a in added.iter() {
            self._removal.remove(a);
            self._insertion.insert(a.clone());
        }

        if !self._insertion.is_empty() {
            if let Some(v) = self._insertion.iter().next() {
                let v = v.clone();
                self._insertion.remove(&v);
                let server_hash = hash_string(&v);
                return Poll::Ready(Some(Ok(Change::Insert(server_hash, v.clone()))));
            }
        } else if !self._removal.is_empty() {
            if let Some(v) = self._removal.iter().next() {
                let v = v.clone();
                self._removal.remove(&v);
                let server_hash = hash_string(&v);
                return Poll::Ready(Some(Ok(Change::Remove(server_hash))));
            }
        }

        Poll::Pending
    }
}
