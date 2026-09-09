use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;

mod build_metadata;

fn main() {
    build_metadata::emit();
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=resources/dark-mode-notify.swift");
    for name in [
        "PATH",
        "DEVELOPER_DIR",
        "SDKROOT",
        "MACOSX_DEPLOYMENT_TARGET",
    ] {
        println!("cargo:rerun-if-env-changed={name}");
    }

    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os != "macos" {
        // WATCHER_BINARY is only referenced behind #[cfg(target_os = "macos")] in
        // src/platform/dark_mode_notify.rs. Bail early on other targets — any non-gated
        // env!("WATCHER_BINARY") elsewhere would fail the build here, which is the signal.
        return;
    }

    let target = std::env::var("TARGET").expect("TARGET not set by cargo");
    let (arch, cpu, minimum) = match target.as_str() {
        "aarch64-apple-darwin" => ("arm64", 0x0100_000c, "11.0"),
        // Swift Foundation needs the system overlays shipped with Catalina.
        // This is the helper's floor, not a change to the Rust executable's floor.
        "x86_64-apple-darwin" => ("x86_64", 0x0100_0007, "10.15"),
        _ => panic!("Unsupported macOS watcher target: {target}"),
    };
    let deployment = match std::env::var("MACOSX_DEPLOYMENT_TARGET") {
        Ok(value) => value,
        Err(std::env::VarError::NotPresent) => minimum.to_owned(),
        Err(error) => panic!("Invalid MACOSX_DEPLOYMENT_TARGET: {error}"),
    };
    assert!(
        version(&deployment).expect("Invalid MACOSX_DEPLOYMENT_TARGET; expected X.Y or X.Y.Z")
            >= version(minimum).unwrap(),
        "MACOSX_DEPLOYMENT_TARGET must be at least {minimum} for the {arch} auto-theme helper"
    );

    let binary_path = PathBuf::from(std::env::var_os("OUT_DIR").expect("OUT_DIR not set by cargo"))
        .join("slate-dark-mode-notify");
    let source = Path::new("resources/dark-mode-notify.swift");
    assert!(source.is_file(), "Swift source file missing");

    // Do not accept an old artifact if a compiler wrapper exits successfully
    // without producing output. OUT_DIR belongs to this package's build script.
    match fs::remove_file(&binary_path) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => panic!("Cannot remove stale watcher artifact: {error}"),
    }
    let mut command = Command::new("swiftc");
    command
        .arg(source)
        .arg("-target")
        .arg(format!("{arch}-apple-macosx{deployment}"))
        .arg("-o")
        .arg(&binary_path);
    if let Some(sdk) = std::env::var_os("SDKROOT").filter(|value| !value.is_empty()) {
        command.arg("-sdk").arg(sdk);
    }

    let result = match command.output() {
        Ok(output) => {
            for line in String::from_utf8_lossy(&output.stderr).lines() {
                println!("cargo:warning=swiftc: {line}");
            }
            if output.status.success() {
                validate_binary(&binary_path, cpu)
            } else {
                Err(format!("swiftc failed with {}", output.status))
            }
        }
        Err(error) => Err(format!("Cannot run swiftc: {error}")),
    };
    if let Err(error) = result {
        if std::env::var("PROFILE").as_deref() != Ok("debug") {
            panic!(
                "Cannot build the macOS auto-theme helper: {error}. \
                 Release builds require a working Xcode Command Line Tools/Swift toolchain."
            );
        }
        println!("cargo:warning={error}");
        println!("cargo:warning=Auto-theme unavailable in this debug build. Install/select Xcode Command Line Tools and rebuild.");
        // A failed compiler may leave a symlink or FIFO. Never follow it while
        // replacing the rejected artifact with the intentionally empty stub.
        match fs::remove_file(&binary_path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => panic!("Cannot remove failed watcher artifact: {error}"),
        }
        fs::write(&binary_path, b"").expect("Failed to create debug stub binary");
    }

    println!("cargo:rustc-env=WATCHER_BINARY={}", binary_path.display());
}

fn version(value: &str) -> Option<[u16; 3]> {
    let parts: Vec<_> = value.split('.').collect();
    if !(2..=3).contains(&parts.len()) {
        return None;
    }
    let mut result = [0; 3];
    for (index, part) in parts.iter().enumerate() {
        if part.is_empty() || !part.bytes().all(|byte| byte.is_ascii_digit()) {
            return None;
        }
        result[index] = part.parse().ok()?;
    }
    Some(result)
}

fn validate_binary(path: &Path, expected_cpu: u32) -> Result<(), String> {
    let metadata =
        fs::symlink_metadata(path).map_err(|error| format!("Watcher output: {error}"))?;
    if !metadata.is_file() {
        return Err("Watcher output must be a regular file".to_owned());
    }
    let mut header = [0; 32];
    fs::File::open(path)
        .and_then(|mut file| file.read_exact(&mut header))
        .map_err(|error| format!("Cannot read watcher Mach-O header: {error}"))?;
    let word = |offset| u32::from_le_bytes(header[offset..offset + 4].try_into().unwrap());
    if word(0) != 0xfeed_facf || word(4) != expected_cpu || word(12) != 2 {
        return Err("Watcher output is not a target-matching 64-bit Mach-O executable".to_owned());
    }
    Ok(())
}
