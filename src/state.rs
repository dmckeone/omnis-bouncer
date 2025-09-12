use reqwest::Client;

use crate::queue::QueueControl;
use crate::upstream::UpstreamPool;

#[derive(Debug)]
pub struct Config {
    pub app_name: String,
    pub cookie_name: String,
    pub header_name: String,
}

// Our app state type
pub struct AppState {
    pub config: Config,
    pub queue: QueueControl,
    pub upstream_pool: UpstreamPool,
    pub client: Client,
}
