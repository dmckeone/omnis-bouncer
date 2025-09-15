use include_dir::{include_dir, Dir};
use std::time::Duration;

// Redis functions
pub static REDIS_FUNCTIONS_DIR: Dir<'static> = include_dir!("$CARGO_MANIFEST_DIR/redis_functions");

// Static directory assets
pub static STATIC_ASSETS_DIR: Dir<'static> = include_dir!("$CARGO_MANIFEST_DIR/static");

// Self-signed certs
pub static SELF_SIGNED_CERT: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/certs/self_cert/server.crt"
));

pub static SELF_SIGNED_KEY: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/certs/self_cert/server.key"
));

// Waiting Room Status
pub static WAITING_ROOM_POSITION_HEADER: &str = "x-queue-position";
pub static WAITING_ROOM_SIZE_HEADER: &str = "x-queue-size";
pub static WAITING_ROOM_POSITION_COOKIE: &str = "queue-position";
pub static WAITING_ROOM_SIZE_COOKIE: &str = "queue-size";

// Web Server
pub static SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(60);

// Error messages
pub static ERROR_NULL_STRING: &str = "<null>";
