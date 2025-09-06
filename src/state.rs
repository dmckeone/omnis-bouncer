use std::sync::Arc;
use tokio::sync::RwLock;

use crate::upstream::UpstreamPool;

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
    pub upstream_pool: Arc<RwLock<UpstreamPool>>,
}
