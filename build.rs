// build.rs

// All queries in the release package go to the server they came from
static API_URI: &str = "/";

fn main() {
    // rebuild if build.rs is changed
    build_deps::rerun_if_changed_paths("build.rs").unwrap();
    build_deps::rerun_if_changed_paths("ui/package.json").unwrap();
    build_deps::rerun_if_changed_paths("ui/pnpm-lock.yaml").unwrap();
    build_deps::rerun_if_changed_paths("ui/vite.config.ts").unwrap();
    build_deps::rerun_if_changed_paths("ui/dist/").unwrap();
    build_deps::rerun_if_changed_paths("ui/src/").unwrap();
    build_deps::rerun_if_changed_paths("ui/src/**").unwrap();
}
