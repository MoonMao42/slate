use slate_cli::env::SlateEnv;
use slate_cli::theme::{ThemeRegistry, FAMILY_SORT_ORDER};
use std::fs::{self, File};
use std::os::fd::AsRawFd;
use std::os::unix::fs::{symlink, PermissionsExt};
use std::path::Path;
use std::process::Output;
use std::time::Duration;
use tempfile::TempDir;

#[path = "support/tree.rs"]
mod tree_snapshot;
use tree_snapshot::tree;

fn run(home: &Path, args: &[&str], success: bool) -> Output {
    let mut command = assert_cmd::Command::new(assert_cmd::cargo::cargo_bin!("slate"));
    command
        .env_clear()
        .env("SLATE_HOME", home)
        .env("PATH", "")
        .env("NO_COLOR", "1")
        .env("TERM", "xterm-ghostty")
        .env("COLORTERM", "truecolor")
        .args(args)
        .timeout(Duration::from_secs(4));
    let assertion = command.assert();
    let assertion = if success {
        assertion.success()
    } else {
        assertion.failure()
    };
    let output = assertion.get_output().clone();
    assert!(
        !output.stdout.contains(&0x1b),
        "unexpected ANSI for {args:?}"
    );
    if success {
        assert!(output.stderr.is_empty(), "{:?}", output.stderr);
    }
    output
}

fn json(home: &Path, args: &[&str]) -> serde_json::Value {
    serde_json::from_slice(&run(home, args, true).stdout).unwrap()
}

fn ids(report: &serde_json::Value) -> Vec<&str> {
    report["themes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|theme| theme["id"].as_str().unwrap())
        .collect()
}

#[test]
fn catalog_formats_share_complete_stable_order_without_initializing_config() {
    let td = TempDir::new().unwrap();
    let before = tree(td.path());
    let report = json(td.path(), &["list", "--json"]);
    assert_eq!(report["schema_version"], 1);
    assert!(report["query"].is_null() && report["appearance"].is_null());
    let registry = ThemeRegistry::new().unwrap();
    let mut expected = registry.all();
    expected.sort_by_key(|theme| {
        (
            FAMILY_SORT_ORDER
                .iter()
                .position(|family| *family == theme.family)
                .unwrap_or(usize::MAX),
            theme.family.as_str(),
        )
    });
    assert_eq!(
        ids(&report),
        expected
            .iter()
            .map(|theme| theme.id.as_str())
            .collect::<Vec<_>>()
    );
    assert_eq!(
        report["count"].as_u64().unwrap() as usize,
        registry.all().len()
    );
    for theme in report["themes"].as_array().unwrap() {
        assert!(matches!(
            theme["appearance"].as_str(),
            Some("dark" | "light")
        ));
        assert!(!theme["name"].as_str().unwrap().is_empty());
        assert!(!theme["family"].as_str().unwrap().is_empty());
        assert!(theme["description"].is_string() || theme["description"].is_null());
        if let Some(pair) = theme["auto_pair"].as_str() {
            assert!(registry.get(pair).is_some());
        }
    }
    let output = run(td.path(), &["list", "--ids", "--quiet"], true);
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        format!("{}\n", ids(&report).join("\n"))
    );
    let text = run(td.path(), &["list"], true).stdout;
    assert_eq!(run(td.path(), &["theme", "--list"], true).stdout, text);
    let text = String::from_utf8(text).unwrap();
    for theme in expected {
        assert!(text.contains(&theme.id));
    }
    assert!(text.contains("[dark]") && text.contains("[light]") && text.contains("◆"));
    // Piped output must be plain even without NO_COLOR and with truecolor advertised.
    let output = assert_cmd::Command::new(assert_cmd::cargo::cargo_bin!("slate"))
        .env_clear()
        .env("SLATE_HOME", td.path())
        .env("PATH", "")
        .env("COLORTERM", "truecolor")
        .env("TERM", "xterm-ghostty")
        .arg("list")
        .timeout(Duration::from_secs(4))
        .assert()
        .success()
        .get_output()
        .clone();
    assert!(!output.stdout.contains(&0x1b));
    assert_eq!(tree(td.path()), before);
}

#[test]
fn catalog_filters_and_argument_errors_are_explicit_and_read_only() {
    let td = TempDir::new().unwrap();
    let before = tree(td.path());
    for (query, appearance, expected) in [
        ("DAWN Rosé", "light", vec!["rose-pine-dawn"]),
        ("catp", "light", vec!["catppuccin-latte"]),
        (
            "catp",
            "dark",
            vec![
                "catppuccin-mocha",
                "catppuccin-frappe",
                "catppuccin-macchiato",
            ],
        ),
        ("no-such-theme", "dark", vec![]),
        ("🌌", "dark", vec![]),
        ("---", "dark", vec![]),
    ] {
        let report = json(
            td.path(),
            &["list", "--json", "--appearance", appearance, "--", query],
        );
        assert_eq!(report["query"], query);
        assert_eq!(report["appearance"], appearance);
        let mut actual = ids(&report);
        let mut expected = expected;
        actual.sort();
        expected.sort();
        assert_eq!(actual, expected);
        assert_eq!(report["count"].as_u64().unwrap() as usize, actual.len());
    }
    assert!(run(td.path(), &["list", "missing", "--ids"], true)
        .stdout
        .is_empty());
    assert!(
        String::from_utf8(run(td.path(), &["list", "missing"], true).stdout)
            .unwrap()
            .contains("No matching themes")
    );
    for args in [
        vec!["list", "--json", "--ids"],
        vec!["list", "--appearance", "dim"],
        vec!["list", "--appearance"],
        vec!["list", "too", "many"],
    ] {
        let output = run(td.path(), &args, false);
        assert!(output.stdout.is_empty());
        assert!(!output.stderr.is_empty());
    }
    let help = run(td.path(), &["list", "--help"], true);
    let help = String::from_utf8(help.stdout).unwrap();
    for flag in ["[QUERY]", "--appearance", "--json", "--ids"] {
        assert!(help.contains(flag), "missing {flag}");
    }
    assert_eq!(tree(td.path()), before);
}

#[test]
fn catalog_stays_available_during_recovery_or_writer_contention() {
    let td = TempDir::new().unwrap();
    let env = SlateEnv::with_home(td.path().to_owned());
    fs::create_dir_all(env.config_dir()).unwrap();
    fs::create_dir_all(env.slate_cache_dir()).unwrap();
    fs::write(
        env.managed_file("config.toml"),
        b"[PRIVATE_BROKEN_PREFERENCES",
    )
    .unwrap();
    let current = env.managed_file("current");
    let c_path = std::ffi::CString::new(current.as_os_str().as_encoded_bytes()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(c_path.as_ptr(), 0o600) }, 0);
    fs::write(
        env.slate_cache_dir().join("preview-session.json"),
        b"PRIVATE_INVALID_RECOVERY",
    )
    .unwrap();
    symlink("missing-file", env.config_dir().join("untouched-link")).unwrap();
    let lock = File::create(env.slate_cache_dir().join("preview-session.lock")).unwrap();
    lock.set_permissions(fs::Permissions::from_mode(0o600))
        .unwrap();
    assert_eq!(
        unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) },
        0
    );
    let before = tree(td.path());
    for args in [
        vec!["list", "--json"],
        vec!["list", "--ids"],
        vec!["list"],
        vec!["theme", "--list"],
        vec!["list", "--auto", "--json"],
    ] {
        let output = run(td.path(), &args, true);
        assert!(!String::from_utf8_lossy(&output.stdout).contains("PRIVATE_"));
        assert_eq!(tree(td.path()), before);
    }
}

#[test]
fn theme_input_failures_precede_all_initialization_and_offer_actionable_feedback() {
    let td = TempDir::new().unwrap();
    let before = tree(td.path());
    for (args, expected) in [
        (vec!["theme", "nrod"], "Suggested IDs: nord"),
        (vec!["set", "catppuccin-mocah"], "catppuccin-mocha"),
        (vec!["theme", "set", "rose dawn"], "rose-pine-dawn"),
        (vec!["theme", "unrelated-theme", "--quiet"], "slate list"),
        (vec!["theme", "set"], "Missing theme"),
        (vec!["theme", "Catppuccin", "Mocha"], "Quote display names"),
        (vec!["theme", "--list", "nord"], "slate list <query>"),
        (vec!["theme", "nord", "--auto"], "cannot be combined"),
        (vec!["--auto", "theme", "set", "nord"], "cannot be combined"),
        (vec!["set", "nord", "--auto"], "cannot be combined"),
        (vec!["--auto", "set", "nord"], "cannot be combined"),
        (vec!["theme", "\x1b[31mUNKNOWN\n\u{202e}"], "not found"),
    ] {
        let output = run(td.path(), &args, false);
        let error = String::from_utf8(output.stderr).unwrap();
        assert!(output.stdout.is_empty(), "{args:?}");
        assert!(error.contains(expected), "{args:?}: {error}");
        assert!(!error.contains('\x1b') && !error.contains('\u{202e}'));
        assert_eq!(
            tree(td.path()),
            before,
            "invalid input created files: {args:?}"
        );
    }
    // Even path configuration is unnecessary for help/version and input errors.
    for args in [vec!["--help"], vec!["--version"], vec!["theme", "--help"]] {
        assert_cmd::Command::new(assert_cmd::cargo::cargo_bin!("slate"))
            .env_clear()
            .args(&args)
            .timeout(Duration::from_secs(4))
            .assert()
            .success();
    }
    assert_cmd::Command::new(assert_cmd::cargo::cargo_bin!("slate"))
        .env_clear()
        .args(["theme", "nrod"])
        .timeout(Duration::from_secs(4))
        .assert()
        .failure()
        .stderr(predicates::str::contains("Suggested IDs: nord"));
    // Direct library callers bypass main's preflight. Run the fixture in a
    // separate, fully isolated process so a regression cannot touch host configs.
    assert_cmd::Command::new(std::env::current_exe().unwrap())
        .env_clear()
        .env("SLATE_HOME", td.path())
        .env("PATH", "")
        .env("NO_COLOR", "1")
        .args([
            "--exact",
            "theme_input_library_fixture",
            "--ignored",
            "--nocapture",
        ])
        .timeout(Duration::from_secs(4))
        .assert()
        .success();
    assert_eq!(tree(td.path()), before);
}

#[test]
#[ignore = "invoked in an isolated child by theme_input_failures_precede_all_initialization_and_offer_actionable_feedback"]
fn theme_input_library_fixture() {
    assert!(std::env::var_os("SLATE_HOME").is_some());
    for auto in [false, true] {
        let expected = slate_cli::cli::theme::validate_selection(Some("nrod"), auto)
            .unwrap_err()
            .to_string();
        assert_eq!(
            slate_cli::cli::theme::handle_theme(Some("nrod".into()), auto, false)
                .unwrap_err()
                .to_string(),
            expected
        );
        assert_eq!(
            slate_cli::cli::set::handle(Some("nrod"), auto, false)
                .unwrap_err()
                .to_string(),
            expected
        );
    }
}

#[test]
fn theme_input_validation_does_not_bypass_mutation_locks_or_read_bad_preferences() {
    let td = TempDir::new().unwrap();
    let env = SlateEnv::with_home(td.path().to_owned());
    fs::create_dir_all(env.config_dir()).unwrap();
    fs::create_dir_all(env.slate_cache_dir()).unwrap();
    let config_path = env.managed_file("config.toml");
    let c_path = std::ffi::CString::new(config_path.as_os_str().as_encoded_bytes()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(c_path.as_ptr(), 0o600) }, 0);
    fs::write(env.managed_file("current"), b"rose-pine-dawn\n").unwrap();
    fs::write(
        env.slate_cache_dir().join("preview-session.json"),
        b"PRIVATE_BAD_RECOVERY",
    )
    .unwrap();
    let lock = File::create(env.slate_cache_dir().join("preview-session.lock")).unwrap();
    lock.set_permissions(fs::Permissions::from_mode(0o600))
        .unwrap();
    assert_eq!(
        unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) },
        0
    );
    let before = tree(td.path());
    for (args, expected) in [
        (vec!["theme", "nrod"], "Suggested IDs: nord"),
        (vec!["theme", "nord", "--auto"], "cannot be combined"),
        (vec!["theme", "Nord"], "still running"),
        (vec!["set", "nord"], "still running"),
        (vec!["theme", "--auto"], "still running"),
        (vec!["set", "--auto"], "still running"),
    ] {
        let error = String::from_utf8(run(td.path(), &args, false).stderr).unwrap();
        assert!(error.contains(expected), "{args:?}: {error}");
        assert!(!error.contains("PRIVATE_"));
        assert_eq!(tree(td.path()), before);
    }
    drop(lock);
    for (args, expected) in [
        (vec!["theme", "nrod"], "Suggested IDs: nord"),
        (vec!["theme", "set", "Nord"], "slate recover --dry-run"),
        (vec!["set", "--auto"], "slate recover --dry-run"),
    ] {
        let error = String::from_utf8(run(td.path(), &args, false).stderr).unwrap();
        assert!(error.contains(expected), "{args:?}: {error}");
        assert_eq!(tree(td.path()), before);
    }
}

#[test]
fn theme_input_complete_names_still_apply_through_all_compatibility_entries() {
    for (args, expected) in [
        (vec!["theme", "Nord", "--quiet"], "nord"),
        (vec!["--quiet", "set", "Rosé Pine Dawn"], "rose-pine-dawn"),
        (
            vec!["theme", "set", "catppuccin-mocha", "--quiet"],
            "catppuccin-mocha",
        ),
    ] {
        let td = TempDir::new().unwrap();
        let env = SlateEnv::with_home(td.path().to_owned());
        let ghostty = env.xdg_config_home().join("ghostty/config.ghostty");
        fs::create_dir_all(ghostty.parent().unwrap()).unwrap();
        fs::write(&ghostty, b"font-size = 15\n").unwrap();
        assert!(run(td.path(), &args, true).stdout.is_empty());
        assert_eq!(
            fs::read_to_string(env.managed_file("current"))
                .unwrap()
                .trim(),
            expected
        );
        assert!(fs::read_to_string(&ghostty)
            .unwrap()
            .contains("font-size = 15"));
    }
}
