use include_dir::{include_dir, Dir};

// Redis functions
pub static REDIS_FUNCTIONS_DIR: Dir<'static> = include_dir!("$CARGO_MANIFEST_DIR/redis_functions");

// Static directory assets
pub static STATIC_ASSETS_DIR: Dir<'static> = include_dir!("$CARGO_MANIFEST_DIR/static");
