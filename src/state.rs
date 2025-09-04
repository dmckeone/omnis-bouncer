use std::sync::RwLock;
use std::sync::{Arc, Mutex};
use std::task::Waker;

#[derive(Debug)]
pub struct Config {
    pub app_name: String,
    pub cookie_name: String,
    pub header_name: String,
}

// Our app state type
#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub upstreams: Arc<RwLock<Vec<String>>>,
    pub upstream_waker: Arc<Mutex<Option<Waker>>>,
}
