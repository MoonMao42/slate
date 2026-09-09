//! Build labels and relocated-artifact inspection, with no user data.
#![cfg(unix)]
use serde_json::Value;
use slate_cli::{config::prompt::PromptStyle, env::SlateEnv, theme::ThemeRegistry};
use std::{
    fs,
    os::unix::{ffi::OsStringExt, fs::PermissionsExt},
    path::Path,
    time::Duration,
};

#[allow(dead_code)] // emit is exercised by watcher_build's real build-script fixture.
#[path = "../build_metadata.rs"]
mod build_metadata;
#[path = "support/tree.rs"]
mod tree_snapshot;

fn seed(path: &Path, bytes: impl AsRef<[u8]>) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
}

fn command() -> assert_cmd::Command {
    let mut cmd = assert_cmd::Command::new(assert_cmd::cargo::cargo_bin!("slate"));
    cmd.env_clear()
        .env("PATH", "")
        .timeout(Duration::from_secs(5));
    cmd
}

fn json(cmd: &mut assert_cmd::Command) -> Value {
    let output = cmd
        .args(["about", "--json"])
        .assert()
        .success()
        .get_output()
        .clone();
    assert!(output.stderr.is_empty());
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn about_reports_embedded_build_and_registries_without_home_or_git() {
    let report = json(&mut command());
    assert_eq!(report["schema_version"], 1);
    assert_eq!(report["scope"], "compiled_capabilities_only");
    assert_eq!(report["build"]["version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(report["build"]["target"], env!("SLATE_BUILD_TARGET"));
    assert_eq!(
        report["build"]["cargo_profile"],
        env!("SLATE_BUILD_PROFILE")
    );
    let tag = report["build"]["source_tag"].as_str().unwrap();
    assert!(regex::Regex::new(r"^fnv1a64-v1-[0-9a-f]{16}$")
        .unwrap()
        .is_match(tag));
    assert_eq!(
        tag,
        build_metadata::source_tag(Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap()
    );
    assert_eq!(
        report["capabilities"]["theme_ids"],
        serde_json::json!(ThemeRegistry::new().unwrap().list_ids())
    );
    assert_eq!(
        report["capabilities"]["tool_adapters"],
        serde_json::json!(slate_cli::cli::tools::supported_tools())
    );
    assert_eq!(
        report["capabilities"]["prompt_styles"],
        serde_json::json!(PromptStyle::ALL.map(PromptStyle::id))
    );
    assert_eq!(
        report["capabilities"]["tool_theme_checks"],
        serde_json::json!([
            "fastfetch",
            "btop",
            "eza",
            "starship",
            "yazi",
            "zellij",
            "lazygit",
            "ghostty",
            "kitty",
            "alacritty"
        ])
    );
    let exe = Path::new(report["executable"]["path"].as_str().unwrap());
    assert_eq!(
        exe.canonicalize().unwrap(),
        assert_cmd::cargo::cargo_bin!("slate")
            .canonicalize()
            .unwrap()
    );
    assert_eq!(report["executable"]["path_is_lossy"], false);
    command()
        .arg("-V")
        .assert()
        .success()
        .stdout(format!("slate {}\n", env!("CARGO_PKG_VERSION")));
    command()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicates::str::contains(tag));
    command()
        .arg("about")
        .assert()
        .success()
        .stdout(predicates::str::contains(
            "not installed tools or live theme status",
        ));
}

#[test]
fn about_ignores_broken_profile_pending_recovery_writer_and_runtime_metadata_overrides() {
    let home = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(home.path().to_owned());
    seed(
        &env.managed_file("config.toml"),
        "[PRIVATE_INVALID_CONFIG\n",
    );
    for tool in ["git", "starship", "btop", "yazi", "zellij", "brew", "uname"] {
        let path = env.user_local_bin().join(tool);
        seed(
            &path,
            "#!/bin/sh\nprintf called > \"$HOME/UNEXPECTED_PROCESS\"\nexit 91\n",
        );
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    }
    let _guard = slate_cli::config::ConfigWriteGuard::acquire(&env).unwrap();
    // Introduce the interrupted journal after acquiring the fixture's writer;
    // ordinary writers correctly refuse to start over an existing journal.
    seed(
        &env.slate_cache_dir().join("preview-session.json"),
        "PRIVATE_BROKEN_RECOVERY",
    );
    let before = tree_snapshot::tree(home.path());
    let expected = json(&mut command());
    let mut cmd = command();
    cmd.env("HOME", home.path())
        .env("SLATE_HOME", home.path())
        .env("PATH", env.user_local_bin())
        .env("YAZI_CONFIG_HOME", "")
        .env("ZELLIJ_CONFIG_DIR", "relative-invalid")
        .env("SLATE_SOURCE_TAG", "PRIVATE_FORGED_BUILD")
        .args(["--auto", "--quiet"]);
    assert_eq!(json(&mut cmd), expected);
    // Even SlateEnv's path validation is bypassed, not merely its writes.
    assert_eq!(
        json(command().env("SLATE_HOME", "relative-invalid")),
        expected
    );
    assert_eq!(tree_snapshot::tree(home.path()), before);
    assert!(!serde_json::to_string(&expected)
        .unwrap()
        .contains("PRIVATE_"));
}

#[test]
fn relocated_binary_reports_its_own_path_and_escapes_terminal_controls() {
    let home = tempfile::tempdir().unwrap();
    let source = json(&mut command());
    // macOS rejects non-UTF-8 filenames; Linux exercises lossy display too.
    // Both platforms still cover Unicode, spaces and terminal escape bytes.
    let mut name = "slate 中文-\x1b[31m-".as_bytes().to_vec();
    if cfg!(target_os = "linux") {
        name.push(0xff);
    }
    let name = std::ffi::OsString::from_vec(name);
    let binary = home.path().join(name);
    fs::copy(assert_cmd::cargo::cargo_bin!("slate"), &binary).unwrap();
    let before = tree_snapshot::tree(home.path());
    let mut cmd = assert_cmd::Command::new(&binary);
    cmd.env_clear()
        .current_dir(home.path())
        .timeout(Duration::from_secs(5));
    let moved = json(&mut cmd);
    assert_eq!(moved["build"], source["build"]);
    assert_eq!(moved["capabilities"], source["capabilities"]);
    assert_eq!(
        moved["executable"]["path_is_lossy"],
        cfg!(target_os = "linux")
    );
    if cfg!(target_os = "linux") {
        assert_eq!(
            moved["executable"]["path"],
            binary.to_string_lossy().as_ref()
        );
    } else {
        // macOS may report /var where canonicalize returns /private/var.
        assert_eq!(
            Path::new(moved["executable"]["path"].as_str().unwrap())
                .canonicalize()
                .unwrap(),
            binary.canonicalize().unwrap()
        );
    }
    let output = assert_cmd::Command::new(&binary)
        .env_clear()
        .current_dir(home.path())
        .arg("about")
        .timeout(Duration::from_secs(5))
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    assert!(!output.contains(&0x1b));
    assert!(String::from_utf8(output).unwrap().contains("\\u{1b}[31m"));
    assert_eq!(tree_snapshot::tree(home.path()), before);
}

fn source_fixture() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    for file in build_metadata::FIXED_INPUTS {
        seed(&root.path().join(file), file);
    }
    seed(&root.path().join("src/main.rs"), "fn main() {}\n");
    root
}

#[test]
fn source_tag_tracks_selected_contents_and_paths_not_locations_or_documentation() {
    let root = source_fixture();
    let other = source_fixture();
    let tag = build_metadata::source_tag(root.path()).unwrap();
    assert_eq!(tag, build_metadata::source_tag(other.path()).unwrap());
    seed(&root.path().join("src/main.rs"), "fn main() {}\n"); // new mtime, same bytes
    for file in [
        "README.md",
        ".git/HEAD",
        "target/debug/ignored.rs",
        "resources/unused.wav",
        "src/notes.md",
    ] {
        seed(&root.path().join(file), "unrelated");
    }
    assert_eq!(tag, build_metadata::source_tag(root.path()).unwrap());
    for file in build_metadata::FIXED_INPUTS
        .iter()
        .copied()
        .chain(["src/main.rs"])
    {
        let path = root.path().join(file);
        let original = fs::read(&path).unwrap();
        seed(&path, [original.clone(), b"changed".to_vec()].concat());
        assert_ne!(
            tag,
            build_metadata::source_tag(root.path()).unwrap(),
            "{file}"
        );
        seed(&path, original);
    }
    let added = root.path().join("src/sub/adapter.rs");
    seed(&added, "// new adapter\n");
    let with_added = build_metadata::source_tag(root.path()).unwrap();
    assert_ne!(tag, with_added);
    let renamed = added.with_file_name("renamed.rs");
    fs::rename(added, &renamed).unwrap();
    assert_ne!(with_added, build_metadata::source_tag(root.path()).unwrap());
    fs::remove_file(renamed).unwrap();
    assert_eq!(tag, build_metadata::source_tag(root.path()).unwrap());
}

#[test]
fn incomplete_or_nonregular_inputs_never_produce_a_partial_source_tag() {
    let root = source_fixture();
    let path = root.path().join("Cargo.lock");
    fs::remove_file(&path).unwrap();
    assert!(build_metadata::source_tag(root.path()).is_err());
    std::os::unix::fs::symlink("Cargo.toml", &path).unwrap();
    assert!(build_metadata::source_tag(root.path()).is_err());
    fs::remove_file(&path).unwrap();
    fs::create_dir(&path).unwrap();
    assert!(build_metadata::source_tag(root.path()).is_err());
    fs::remove_dir(&path).unwrap();
    fs::File::create(&path)
        .unwrap()
        .set_len(64 * 1024 * 1024 + 1)
        .unwrap();
    assert!(build_metadata::source_tag(root.path()).is_err());
    fs::write(&path, "lock").unwrap();
    std::os::unix::fs::symlink("../src", root.path().join("src/linked")).unwrap();
    assert!(build_metadata::source_tag(root.path()).is_err());
}
