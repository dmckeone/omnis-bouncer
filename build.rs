use build_deps::rerun_if_changed_paths;
use std::{env::var, path::Path, process::Command};

// Cargo doesn't output normal println!() so use this small macro to print limited output
macro_rules! log {
    ($($tokens: tt)*) => {
        println!("cargo:warning={}", format!($($tokens)*))
    }
}

fn main() {
    let build_ui = match var("OMNIS_BOUNCER_BUILD_UI") {
        Ok(value) => !["0", "false", "f"].contains(&value.to_lowercase().as_str()),
        Err(_) => true,
    };

    let profile = match var("PROFILE") {
        Ok(profile) => profile,
        Err(e) => {
            log!("Unable to find PROFILE environment variable: {}", e);
            return;
        }
    };

    // Only build UI on release builds
    if !build_ui || profile != "release" {
        return;
    }

    log!("Building UI: pnpm run build");

    let ui_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("ui");
    let output = if cfg!(target_os = "windows") {
        Command::new("cmd")
            .current_dir(&ui_dir)
            .args(["/C", "pnpm run build"])
            .output()
            .expect("failed to build user interface with pnpm")
    } else {
        Command::new("sh")
            .current_dir(&ui_dir)
            .arg("-c")
            .arg("pnpm run build")
            .output()
            .expect("failed to build user interface with pnpm")
    };
    if !output.status.success() {
        log!("pnpm failed - exit code: {:?}", output.status.code());
        if !output.stdout.is_empty() {
            log!(
                "pnpm output: {:?}",
                String::from_utf8(output.stdout).unwrap()
            );
        }
        if !output.stderr.is_empty() {
            log!(
                "pnpm error: {:?}",
                String::from_utf8(output.stderr).unwrap()
            );
        }
    }

    // Hint to cargo which paths indicate a rebuild
    rerun_if_changed_paths("build.rs").unwrap();
    rerun_if_changed_paths("ui/package.json").unwrap();
    rerun_if_changed_paths("ui/pnpm-lock.yaml").unwrap();
    rerun_if_changed_paths("ui/vite.config.ts").unwrap();
    rerun_if_changed_paths("ui/src").unwrap();
    rerun_if_changed_paths("ui/src/**").unwrap();
}
