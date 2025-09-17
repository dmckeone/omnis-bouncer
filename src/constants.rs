use include_dir::{include_dir, Dir};
use std::time::Duration;

// Redis functions
pub static REDIS_FUNCTIONS_DIR: Dir<'static> = include_dir!("$CARGO_MANIFEST_DIR/redis_functions");

// Static directory assets
pub static STATIC_ASSETS_DIR: Dir<'static> = include_dir!("$CARGO_MANIFEST_DIR/static");

// Control Web UI
pub static UI_INDEX: &[u8] =
    include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/ui/dist/index.html"));
pub static UI_FAVICON: &[u8] =
    include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/ui/dist/favicon.ico"));
pub static UI_ASSET_DIR: Dir<'static> = include_dir!("$CARGO_MANIFEST_DIR/ui/dist/assets");

// Self-signed certs
pub static SELF_SIGNED_CERT: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/certs/self_cert/server.crt"
));

pub static SELF_SIGNED_KEY: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/certs/self_cert/server.key"
));

// Background
pub static BACKGROUND_SLEEP_TIME: Duration = Duration::from_secs(30);

// Web Server
pub static SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(60);

// Error messages
pub static ERROR_NULL_STRING: &str = "<null>";
