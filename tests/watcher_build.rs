//! Exercise the real build script with disposable compiler fixtures, not Xcode.
#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::OnceLock;
use std::time::Duration;
use tempfile::TempDir;

const ARM: u32 = 0x0100_000c;
const INTEL: u32 = 0x0100_0007;

fn build_script() -> &'static Path {
    static SCRIPT: OnceLock<(TempDir, PathBuf)> = OnceLock::new();
    &SCRIPT
        .get_or_init(|| {
            let directory = TempDir::new().unwrap();
            let binary = directory.path().join("build-script");
            assert_cmd::Command::new(std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into()))
                .current_dir(env!("CARGO_MANIFEST_DIR"))
                .args(["--edition=2021", "build.rs", "-o"])
                .arg(&binary)
                .timeout(Duration::from_secs(30))
                .assert()
                .success();
            (directory, binary)
        })
        .1
}

fn macho_header(cpu: u32, file_type: u32) -> Vec<u8> {
    let mut bytes = vec![0; 32];
    bytes[0..4].copy_from_slice(&0xfeed_facfu32.to_le_bytes());
    bytes[4..8].copy_from_slice(&cpu.to_le_bytes());
    bytes[12..16].copy_from_slice(&file_type.to_le_bytes());
    bytes
}

struct Fixture {
    directory: TempDir,
    bin: PathBuf,
    out: PathBuf,
    payload: PathBuf,
    arguments: PathBuf,
}

impl Fixture {
    fn new(cpu: u32) -> Self {
        let directory = TempDir::new().unwrap();
        let bin = directory.path().join("compiler bin");
        let out = directory.path().join("build output");
        fs::create_dir(&bin).unwrap();
        fs::create_dir(&out).unwrap();
        let compiler = bin.join("swiftc");
        fs::write(
            &compiler,
            r#"#!/bin/sh
printf '%s\n' "$@" > "$ARG_LOG"
while [ "$#" -gt 0 ]; do
    case "$1" in
        -o) shift; out=$1 ;;
    esac
    shift
done
case "$FAKE_SWIFT_MODE" in
    fail) printf 'first diagnostic\nsecond diagnostic\n' >&2; exit 7 ;;
    no-output) exit 0 ;;
    empty) : > "$out" ;;
    symlink) /bin/ln -s "$PAYLOAD" "$out" ;;
    *) /bin/cp "$PAYLOAD" "$out" ;;
esac
"#,
        )
        .unwrap();
        fs::set_permissions(&compiler, fs::Permissions::from_mode(0o755)).unwrap();
        let payload = directory.path().join("payload");
        fs::write(&payload, macho_header(cpu, 2)).unwrap();
        let arguments = directory.path().join("arguments");
        Self {
            directory,
            bin,
            out,
            payload,
            arguments,
        }
    }

    fn command(&self, target: &str, profile: &str, mode: &str) -> assert_cmd::Command {
        let mut command = Command::new(build_script());
        command
            .current_dir(env!("CARGO_MANIFEST_DIR"))
            .env_clear()
            .env("PATH", &self.bin)
            .env("CARGO_CFG_TARGET_OS", "macos")
            .env("TARGET", target)
            .env("PROFILE", profile)
            .env("OUT_DIR", &self.out)
            .env("FAKE_SWIFT_MODE", mode)
            .env("PAYLOAD", &self.payload)
            .env("ARG_LOG", &self.arguments);
        let mut command = assert_cmd::Command::from_std(command);
        command.timeout(Duration::from_secs(5));
        command
    }

    fn artifact(&self) -> PathBuf {
        self.out.join("slate-dark-mode-notify")
    }
}

fn stdout(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).unwrap()
}

#[test]
fn build_metadata_is_available_without_git_and_degrades_without_source_inputs() {
    let fixture = Fixture::new(INTEL);
    let output = fixture
        .command("x86_64-unknown-linux-gnu", "release", "fail")
        .env("CARGO_CFG_TARGET_OS", "linux")
        .env("CARGO_FEATURE_HAS_NVIM", "1")
        .env("CARGO_FEATURE_HAS_FISH", "1")
        .assert()
        .success()
        .get_output()
        .clone();
    let text = stdout(&output);
    assert!(text.contains("cargo:rustc-env=SLATE_SOURCE_TAG=fnv1a64-v1-"));
    assert!(text.contains("cargo:rustc-env=SLATE_BUILD_TARGET=x86_64-unknown-linux-gnu\n"));
    assert!(text.contains("cargo:rustc-env=SLATE_BUILD_PROFILE=release\n"));
    assert!(text.contains("cargo:rustc-env=SLATE_BUILD_FEATURES=has-fish,has-nvim\n"));
    for path in [
        "src",
        "Cargo.lock",
        "build_metadata.rs",
        "themes/themes.toml",
        "resources/sfx/click.wav",
    ] {
        assert!(text.contains(&format!("cargo:rerun-if-changed={path}\n")));
    }
    let output = fixture
        .command("invalid\ncargo:rustc-env=INJECTED=1", "debug", "fail")
        .env("CARGO_CFG_TARGET_OS", "linux")
        .env("CARGO_MANIFEST_DIR", fixture.directory.path())
        .assert()
        .success()
        .get_output()
        .clone();
    let text = stdout(&output);
    assert!(text.contains("cargo:rustc-env=SLATE_SOURCE_TAG=unavailable\n"));
    assert!(text.contains("cargo:rustc-env=SLATE_BUILD_TARGET=unknown\n"));
    assert!(!text.contains("INJECTED"));
    assert!(!fixture.arguments.exists());
    assert!(!fixture.artifact().exists());
}

#[test]
fn watcher_build_uses_explicit_target_and_preserves_sdk_argument_boundaries() {
    for (cargo_target, cpu, deployment, expected) in [
        ("aarch64-apple-darwin", ARM, None, "arm64-apple-macosx11.0"),
        (
            "x86_64-apple-darwin",
            INTEL,
            None,
            "x86_64-apple-macosx10.15",
        ),
        (
            "aarch64-apple-darwin",
            ARM,
            Some("15.3.1"),
            "arm64-apple-macosx15.3.1",
        ),
    ] {
        let fixture = Fixture::new(cpu);
        let sdk = fixture.directory.path().join("SDK with spaces");
        let mut command = fixture.command(cargo_target, "release", "success");
        command.env("SDKROOT", &sdk);
        if let Some(deployment) = deployment {
            command.env("MACOSX_DEPLOYMENT_TARGET", deployment);
        }
        let output = command.assert().success().get_output().clone();
        let arguments = fs::read_to_string(&fixture.arguments).unwrap();
        assert_eq!(
            arguments.lines().collect::<Vec<_>>(),
            [
                "resources/dark-mode-notify.swift",
                "-target",
                expected,
                "-o",
                fixture.artifact().to_str().unwrap(),
                "-sdk",
                sdk.to_str().unwrap(),
            ]
        );
        assert!(stdout(&output).contains("cargo:rustc-env=WATCHER_BINARY="));
        for name in [
            "PATH",
            "DEVELOPER_DIR",
            "SDKROOT",
            "MACOSX_DEPLOYMENT_TARGET",
        ] {
            assert!(stdout(&output).contains(&format!("cargo:rerun-if-env-changed={name}")));
        }
        assert_eq!(fs::read(fixture.artifact()).unwrap(), macho_header(cpu, 2));
    }
}

#[test]
fn watcher_build_rejects_failed_missing_stale_and_wrong_target_release_artifacts() {
    for mode in [
        "fail",
        "missing-compiler",
        "no-output",
        "empty",
        "wrong-cpu",
        "not-executable",
        "not-macho",
        "symlink",
    ] {
        let fixture = Fixture::new(ARM);
        // A prior successful build must never mask this build's failure.
        fs::write(fixture.artifact(), macho_header(ARM, 2)).unwrap();
        match mode {
            "missing-compiler" => fs::remove_file(fixture.bin.join("swiftc")).unwrap(),
            "wrong-cpu" => fs::write(&fixture.payload, macho_header(INTEL, 2)).unwrap(),
            "not-executable" => fs::write(&fixture.payload, macho_header(ARM, 6)).unwrap(),
            "not-macho" => fs::write(&fixture.payload, [0; 32]).unwrap(),
            _ => {}
        }
        let output = fixture
            .command("aarch64-apple-darwin", "release", mode)
            .assert()
            .failure()
            .get_output()
            .clone();
        assert!(String::from_utf8_lossy(&output.stderr).contains("Cannot build the macOS"));
        assert!(!stdout(&output).contains("cargo:rustc-env=WATCHER_BINARY="));
        if mode == "fail" {
            assert!(stdout(&output).contains("cargo:warning=swiftc: first diagnostic\n"));
            assert!(stdout(&output).contains("cargo:warning=swiftc: second diagnostic\n"));
        }
    }
    let fixture = Fixture::new(ARM);
    fixture
        .command("aarch64-apple-darwin", "dist", "fail")
        .assert()
        .failure();
}

#[test]
fn watcher_build_retains_visible_debug_fallback_and_skips_swift_on_linux() {
    for mode in ["missing-compiler", "fail", "no-output", "empty", "symlink"] {
        let fixture = Fixture::new(ARM);
        fs::write(fixture.artifact(), macho_header(ARM, 2)).unwrap();
        if mode == "missing-compiler" {
            fs::remove_file(fixture.bin.join("swiftc")).unwrap();
        }
        let output = fixture
            .command("aarch64-apple-darwin", "debug", mode)
            .assert()
            .success()
            .get_output()
            .clone();
        assert!(fs::symlink_metadata(fixture.artifact()).unwrap().is_file());
        assert!(fs::read(fixture.artifact()).unwrap().is_empty());
        assert_eq!(fs::read(&fixture.payload).unwrap(), macho_header(ARM, 2));
        assert!(
            stdout(&output).contains("cargo:warning=Auto-theme unavailable in this debug build.")
        );
        assert!(stdout(&output).contains("cargo:rustc-env=WATCHER_BINARY="));
    }

    let fixture = Fixture::new(INTEL);
    let output = fixture
        .command("x86_64-unknown-linux-gnu", "release", "fail")
        .env("CARGO_CFG_TARGET_OS", "linux")
        .env_remove("OUT_DIR")
        .assert()
        .success()
        .get_output()
        .clone();
    assert!(!fixture.arguments.exists());
    assert!(!fixture.artifact().exists());
    assert!(!stdout(&output).contains("WATCHER_BINARY="));
}

#[test]
fn watcher_build_rejects_unsupported_targets_and_invalid_or_too_old_deployment() {
    for (target, deployment) in [
        ("arm64e-apple-darwin", None),
        ("aarch64-apple-darwin", Some("10.15")),
        ("x86_64-apple-darwin", Some("10.14.4")),
        ("aarch64-apple-darwin", Some("")),
        ("aarch64-apple-darwin", Some("11.0\n27.0")),
        ("aarch64-apple-darwin", Some("11.0;false")),
        ("aarch64-apple-darwin", Some("11.0.0.1")),
    ] {
        let fixture = Fixture::new(ARM);
        let mut command = fixture.command(target, "debug", "success");
        if let Some(deployment) = deployment {
            command.env("MACOSX_DEPLOYMENT_TARGET", deployment);
        }
        command.assert().failure();
        assert!(!fixture.arguments.exists());
        assert!(!fixture.artifact().exists());
    }
}
