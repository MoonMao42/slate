//! Noninteractive font listing must never select, install, initialize or refresh.
use slate_cli::{config::ConfigWriteGuard, env::SlateEnv};
use std::{
    fs,
    os::fd::OwnedFd,
    os::unix::{
        fs::{symlink, PermissionsExt},
        net::UnixStream,
    },
    path::Path,
    process::Stdio,
    time::Duration,
};

#[path = "support/redirected_output.rs"]
mod redirected_output;
#[path = "support/tree.rs"]
mod snapshot;

const FONT: &[u8] = b"\0\x01\0\0private-font-list-fixture";
fn write(path: &Path, bytes: impl AsRef<[u8]>) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
}
fn command(env: &SlateEnv, isolated: bool) -> assert_cmd::Command {
    let mut command = assert_cmd::Command::new(assert_cmd::cargo::cargo_bin!("slate"));
    command
        .env_clear()
        .env("HOME", env.home())
        .env("PATH", env.home().join("bin"))
        .env("XDG_CONFIG_HOME", env.xdg_config_home())
        .env("XDG_DATA_HOME", env.xdg_data_home())
        .env("XDG_CACHE_HOME", env.cache_dir())
        .env("NO_COLOR", "1")
        .current_dir(env.home())
        .timeout(Duration::from_secs(10));
    if isolated {
        command.env("SLATE_HOME", env.home());
    }
    command
}
fn json(command: &mut assert_cmd::Command) -> serde_json::Value {
    let output = command
        .args(["font", "--list", "--json"])
        .assert()
        .success()
        .get_output()
        .clone();
    assert!(
        output.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(!text.contains("PRIVATE_CONTENT") && !text.contains("UNEXPECTED_TOOL"));
    let report: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(report["schema_version"], 1);
    report
}

fn search(env: &SlateEnv, query: &str) -> serde_json::Value {
    let output = command(env, true)
        .args(["font", "--list", "--json"])
        .arg(format!("--search={query}"))
        .assert()
        .success()
        .get_output()
        .clone();
    assert!(output.stderr.is_empty());
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn font_list_search_cli_filters_only_the_view_and_keeps_busy_partial_scan_evidence() {
    let home = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(home.path().into());
    seed(&env);
    let _guard = ConfigWriteGuard::acquire(&env).unwrap();
    write(&env.managed_file("config.toml"), "PRIVATE_CONTENT");
    write(
        &env.slate_cache_dir().join("preview-session.json"),
        "PRIVATE_CONTENT",
    );
    let root = slate_cli::platform::fonts::user_font_dir(&env);
    symlink(root.join("missing"), root.join("bad-link")).unwrap();
    let before = snapshot::tree(home.path());
    let original = json(&mut command(&env, true));
    assert!(!original.as_object().unwrap().contains_key("search"));
    for query in [
        "mono JETBRAINS",
        "jetbrains-mono",
        "",
        "---",
        "no-such-fixture-font",
    ] {
        let report = search(&env, query);
        assert_eq!(report["schema_version"], 1);
        assert_eq!(report["scan_complete"], false);
        assert_eq!(report["search"]["query"], query);
        assert_eq!(
            report["search"]["total_candidates"],
            original["candidates"].as_array().unwrap().len()
        );
        assert_eq!(
            report["search"]["matched_candidates"],
            report["candidates"].as_array().unwrap().len()
        );
        for field in ["search_roots", "scan_issues", "omitted_issue_count"] {
            assert_eq!(report[field], original[field]);
        }
        for entry in report["catalog"].as_array().unwrap() {
            assert!(original["catalog"].as_array().unwrap().contains(entry));
            assert_eq!(entry["download_offered"], false);
        }
        if query.is_empty() {
            assert_eq!(report["candidates"], original["candidates"]);
            assert_eq!(report["catalog"], original["catalog"]);
        } else if query.contains("jetbrains") || query.contains("JETBRAINS") {
            assert!(report["candidates"]
                .as_array()
                .unwrap()
                .iter()
                .any(|entry| entry["family"] == "JetBrainsMono Nerd Font"));
            assert!(report["candidates"]
                .as_array()
                .unwrap()
                .iter()
                .all(|entry| entry["family"]
                    .as_str()
                    .unwrap()
                    .to_lowercase()
                    .contains("jetbrains")));
        } else {
            assert!(report["candidates"].as_array().unwrap().is_empty());
            assert!(report["catalog"].as_array().unwrap().is_empty());
        }
        assert_eq!(snapshot::tree(home.path()), before);
    }
    let output = command(&env, true)
        .args(["font", "--list", "--search", "\x1b[2J\u{202e}"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8(output).unwrap();
    assert!(text.contains("No candidates match this search"));
    assert!(!text.contains('\x1b') && !text.contains('\u{202e}'));
    assert_eq!(snapshot::tree(home.path()), before);
    assert!(!env.home().join("tool-was-launched").exists());
}
fn seed(env: &SlateEnv) {
    let root = slate_cli::platform::fonts::user_font_dir(env);
    for name in [
        "JetBrainsMonoNerdFont-Regular.TTF",
        "JetBrainsMonoNerdFontMono-Regular.ttf",
        "JetBrainsMonoNerdFontPropo-Regular.otf",
        "SlateListFixtureNerdFont-Regular.ttc",
    ] {
        write(&root.join("nested").join(name), FONT);
    }
    for tool in [
        "fc-cache",
        "fc-list",
        "fc-match",
        "brew",
        "curl",
        "ghostty",
        "kitten",
        "osascript",
    ] {
        let path = env.home().join("bin").join(tool);
        write(
            &path,
            "#!/bin/sh\n: > \"$HOME/tool-was-launched\"\nprintf UNEXPECTED_TOOL >&2\nexit 92\n",
        );
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    }
}

#[test]
fn font_list_formats_preserve_all_variants_without_initializing_or_launching() {
    let home = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(home.path().into());
    let before = snapshot::tree(home.path());
    let empty = json(&mut command(&env, true));
    assert_eq!(empty["catalog"].as_array().unwrap().len(), 4);
    assert_eq!(snapshot::tree(home.path()), before);
    seed(&env);
    let before = snapshot::tree(home.path());
    let report = json(&mut command(&env, true));
    let text = command(&env, true)
        .args(["font", "--list"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8(text).unwrap();
    assert!(!text.contains('\u{1b}'));
    for family in [
        "JetBrainsMono Nerd Font",
        "JetBrainsMono Nerd Font Mono",
        "JetBrainsMono Nerd Font Propo",
        "SlateListFixture Nerd Font",
    ] {
        assert_eq!(
            report["candidates"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|candidate| candidate["family"] == family)
                .count(),
            1
        );
        assert!(text.contains(family));
    }
    assert!(!text.contains("Select font:"));
    assert_eq!(snapshot::tree(home.path()), before);
    assert!(!env.config_dir().exists() && !env.slate_cache_dir().exists());
}

#[test]
fn font_list_remains_read_only_with_busy_writer_invalid_settings_and_pending_recovery() {
    let home = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(home.path().into());
    seed(&env);
    let guard = ConfigWriteGuard::acquire(&env).unwrap();
    write(&env.managed_file("current-font"), b"PRIVATE_CONTENT\xff");
    write(
        &env.slate_cache_dir().join("preview-session.json"),
        "PRIVATE_CONTENT",
    );
    let path = env.managed_file("config.toml");
    let path = std::ffi::CString::new(path.as_os_str().as_encoded_bytes()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(path.as_ptr(), 0o600) }, 0);
    let before = snapshot::tree(home.path());
    let report = json(command(&env, true).args(["--auto", "--quiet"]));
    assert!(!report["candidates"].as_array().unwrap().is_empty());
    // The new list route must not accidentally exempt font mutations.
    command(&env, true)
        .args(["font", "SlateListFixture Nerd Font"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("still running"));
    assert_eq!(snapshot::tree(home.path()), before);
    drop(guard);
    assert_eq!(json(&mut command(&env, true)), report);
    assert_eq!(snapshot::tree(home.path()), before);
}

#[test]
fn font_list_partial_scan_keeps_positive_evidence_and_unknown_catalog_entries() {
    let home = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(home.path().into());
    seed(&env);
    let root = slate_cli::platform::fonts::user_font_dir(&env);
    symlink(root.join("missing"), root.join("bad\n\u{202e}root")).unwrap();
    let before = snapshot::tree(home.path());
    let report = json(&mut command(&env, true));
    assert_eq!(report["scan_complete"], false);
    assert!(!report["scan_issues"].as_array().unwrap().is_empty());
    for entry in report["catalog"].as_array().unwrap() {
        assert_eq!(entry["download_offered"], false);
        assert!(matches!(
            entry["presence"].as_str(),
            Some("candidate_found" | "unknown")
        ));
        if entry["id"] == "jetbrains-mono" {
            assert_eq!(entry["presence"], "candidate_found");
            assert!(entry["matching_candidates"]
                .as_array()
                .unwrap()
                .iter()
                .any(|name| name == "JetBrainsMono Nerd Font"));
        }
    }
    let output = command(&env, true)
        .args(["font", "--list"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8(output).unwrap();
    assert!(text.contains("incomplete"));
    assert!(!text.contains('\u{1b}') && !text.contains('\u{202e}'));
    assert_eq!(snapshot::tree(home.path()), before);
}

#[test]
fn font_list_honors_profile_paths_and_excludes_host_xdg_overrides_when_isolated() {
    for isolated in [false, true] {
        let home = tempfile::tempdir().unwrap();
        let external = tempfile::tempdir().unwrap();
        let env = SlateEnv::from_vars(|key| match key {
            "HOME" => Some(home.path().into()),
            "SLATE_HOME" if isolated => Some(home.path().into()),
            "XDG_DATA_HOME" => Some(external.path().join("字 data").into_os_string()),
            _ => None,
        })
        .unwrap();
        seed(&env);
        let before = snapshot::tree(home.path());
        let external_before = snapshot::tree(external.path());
        let report =
            json(command(&env, isolated).env("XDG_DATA_HOME", external.path().join("字 data")));
        assert_eq!(
            report["search_roots"][0]["path"],
            slate_cli::platform::fonts::user_font_dir(&env)
                .to_str()
                .unwrap()
        );
        assert!(report["candidates"]
            .as_array()
            .unwrap()
            .iter()
            .any(|font| font["family"] == "SlateListFixture Nerd Font"));
        assert_eq!(snapshot::tree(home.path()), before);
        assert_eq!(snapshot::tree(external.path()), external_before);
        if isolated {
            assert!(!report
                .to_string()
                .contains(external.path().to_str().unwrap()));
        }
    }
}

#[test]
fn font_list_argument_errors_and_closed_output_never_mutate_or_panic() {
    use assert_cmd::assert::OutputAssertExt;
    let home = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(home.path().into());
    seed(&env);
    let before = snapshot::tree(home.path());
    for args in [
        vec!["font", "--json"],
        vec!["font", "--list", "--", "Some Family"],
        vec!["font", "Some Family", "--json"],
        vec!["font", "--search", "mono"],
        vec!["font", "--list", "--search"],
        vec!["font", "--list", "--search", "--json"],
        vec!["font", "Hack", "--dry-run", "--search", "mono"],
    ] {
        // Invalid combinations must be rejected by clap before needing HOME.
        assert_cmd::Command::new(assert_cmd::cargo::cargo_bin!("slate"))
            .env_clear()
            .args(args)
            .timeout(Duration::from_secs(3))
            .assert()
            .failure();
    }
    assert_cmd::Command::new(assert_cmd::cargo::cargo_bin!("slate"))
        .env_clear()
        .args(["font", "--list", "--search", &"x".repeat(257)])
        .timeout(Duration::from_secs(3))
        .assert()
        .failure()
        .stderr(predicates::str::contains("at most 256 bytes"));
    let (reader, writer) = UnixStream::pair().unwrap();
    drop(reader);
    let mut closed = std::process::Command::new(assert_cmd::cargo::cargo_bin!("slate"));
    closed
        .env_clear()
        .env("SLATE_HOME", env.home())
        .env("PATH", "")
        .args(["font", "--list", "--search", "mono", "--json"])
        .stdout(Stdio::from(OwnedFd::from(writer)))
        .stderr(Stdio::piped());
    redirected_output::run(&mut closed)
        .assert()
        .success()
        .stderr("");
    assert_eq!(snapshot::tree(home.path()), before);
}
