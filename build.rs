use std::path::PathBuf;
use std::process::Command;

fn main() {
    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR not set by cargo");
    let out_path = PathBuf::from(&out_dir);

    // Read the Swift source
    let swift_source_path = "resources/dark-mode-notify.swift";

    if !std::path::Path::new(swift_source_path).exists() {
        eprintln!("Swift source file not found at {}", swift_source_path);
        panic!("Swift source file missing");
    }

    // Output binary path
    let binary_path = out_path.join("slate-dark-mode-notify");

    // Compile the Swift binary (requires swiftc from Xcode or Command Line Tools)
    let swiftc_result = Command::new("swiftc")
        .arg(swift_source_path)
        .arg("-o")
        .arg(&binary_path)
        .output();

    match swiftc_result {
        Ok(output) if output.status.success() && binary_path.exists() => {
            // Successfully compiled
        }
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            eprintln!("cargo:warning=swiftc compilation failed: {}", stderr);
            eprintln!("cargo:warning=Auto-theme (dark mode watcher) will not be available.");
            // Create a minimal stub so include_bytes! doesn't fail
            std::fs::write(&binary_path, b"").expect("Failed to create stub binary");
        }
        Err(e) => {
            eprintln!("cargo:warning=swiftc not found ({}). Install Xcode Command Line Tools for auto-theme support.", e);
            // Create a minimal stub so include_bytes! doesn't fail
            std::fs::write(&binary_path, b"").expect("Failed to create stub binary");
        }
    }

    println!("cargo:rustc-env=WATCHER_BINARY={}", binary_path.display());
}
