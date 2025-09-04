use include_dir::{Dir, include_dir};

// Static directory assets
pub static STATIC_ASSETS_DIR: Dir<'static> = include_dir!("$CARGO_MANIFEST_DIR/static");
