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

    // Compile the Swift binary
    let output = Command::new("swiftc")
        .arg(swift_source_path)
        .arg("-o")
        .arg(&binary_path)
        .output()
        .expect("Failed to run swiftc");

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        eprintln!("swiftc compilation failed: {}", stderr);
        panic!("Failed to compile Swift watcher binary");
    }

    // Ensure the binary was created
    if !binary_path.exists() {
        panic!("Swift watcher binary was not created at {:?}", binary_path);
    }

    println!("cargo:rustc-env=WATCHER_BINARY={}", binary_path.display());
}
