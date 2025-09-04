use std::sync::Arc;

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
}
