use std::sync::Arc;

use crate::queue::QueueControl;
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
    pub queue: Arc<QueueControl>,
    pub upstream_pool: Arc<UpstreamPool>,
}
