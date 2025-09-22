// build.rs

use npm_rs::{NodeEnv, NpmEnv};

// All queries in the release package go to the server they came from
static API_URI: &str = "/";

fn main() {
    let _exit_status = NpmEnv::default()
        .with_node_env(&NodeEnv::from_cargo_profile().unwrap_or_default())
        .with_env("VITE_API_URI", API_URI) // Ensure that all API requests got to the root to avoid CORS errors
        .set_path("ui")
        .init_env()
        .install(None)
        .run("build")
        .exec()
        .unwrap();

    // rebuild if build.rs is changed
    build_deps::rerun_if_changed_paths("build.rs").unwrap();
    build_deps::rerun_if_changed_paths("ui/package.json").unwrap();
    build_deps::rerun_if_changed_paths("ui/vite.config.ts").unwrap();
    build_deps::rerun_if_changed_paths("ui/dist/").unwrap();
    build_deps::rerun_if_changed_paths("ui/src/").unwrap();
    build_deps::rerun_if_changed_paths("ui/src/**").unwrap();
}
