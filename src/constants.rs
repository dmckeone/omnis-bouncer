use include_dir::{Dir, include_dir};
use std::time::Duration;

// Redis functions
pub static REDIS_FUNCTIONS_DIR: Dir<'static> = include_dir!("$CARGO_MANIFEST_DIR/redis_functions");

// Static directory assets
pub static STATIC_ASSETS_DIR: Dir<'static> = include_dir!("$CARGO_MANIFEST_DIR/static");

// HTML Template directory
pub static HTML_TEMPLATE_DIR: Dir<'static> = include_dir!("$CARGO_MANIFEST_DIR/html");
pub static DEFAULT_WAITING_ROOM_PAGE: &str = "default_waiting_room.html";

// Control Web UI
pub static UI_INDEX: &[u8] =
    include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/ui/dist/index.html"));
pub static UI_FAVICON: &[u8] =
    include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/ui/dist/favicon.ico"));
pub static UI_ASSET_DIR: Dir<'static> = include_dir!("$CARGO_MANIFEST_DIR/ui/dist/assets");

// CA certs
pub static AUTHORITY_CERT: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/certs/self_authority/root.crt"
));

pub static AUTHORITY_PFX: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/certs/self_authority/root.pfx"
));

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
pub static BACKGROUND_SLEEP_TIME: Duration = Duration::from_secs(10);

// Web Server
pub static DEBOUNCE_INTERVAL: Duration = Duration::from_secs(2);
pub static SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(60);

// Web Server Debug
#[cfg(debug_assertions)]
pub static LOCALHOST_CORS_DEBUG_URI: &str = "http://localhost:5173";
