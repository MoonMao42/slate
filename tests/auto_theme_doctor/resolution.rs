use super::*;
use serde_json::Value;
use slate_cli::config::ConfigWriteGuard;

fn fixture(home: &Path) -> SlateEnv {
    for name in [
        "defaults",
        "gsettings",
        "pgrep",
        "ps",
        "nvim",
        "starship",
        "fastfetch",
        "osascript",
        "fc-list",
        "brew",
    ] {
        write(
            &home.join("bin").join(name),
            "#!/bin/sh\nprintf native > \"$HOME/UNEXPECTED\"\nexit 99\n",
            0o700,
        );
    }
    SlateEnv::with_home(home.into())
}

fn compare(home: &Path, errors: bool) -> Value {
    let before = tree(home);
    let report = inspect(home);
    let output = command(home)
        .args(["config", "pairing", "--json"])
        .assert()
        .success()
        .get_output()
        .clone();
    assert!(output.stderr.is_empty());
    assert!(!String::from_utf8_lossy(&output.stdout).contains("PRIVATE_FIXTURE_CONTENT"));
    let pairing: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["schema_version"], 1);
    assert_eq!(report["resolution"], pairing["resolution"]);
    assert_eq!(report["auto_theme_enabled"], false);
    assert_eq!(report["runtime"]["state"], "absent");
    assert_eq!(report["issues"].as_array().unwrap().is_empty(), !errors);
    let output = command(home)
        .args(["doctor", "auto-theme"])
        .assert()
        .success()
        .get_output()
        .clone();
    assert!(output.stderr.is_empty());
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(text.contains("Automatic choices (conditional; no desktop query):"));
    assert!(text
        .contains("not a desktop query, atomic configuration snapshot or apply-readiness check"));
    assert!(!text.contains("PRIVATE_FIXTURE_CONTENT"));
    if errors {
        assert!(text.contains("slate config pairing"));
        assert!(text.contains("--clear-dark/--clear-light"));
    }
    assert_eq!(tree(home), before, "read-only commands changed the fixture");
    report
}

#[test]
fn auto_theme_doctor_resolution_matches_pairing_without_probing_or_treating_fallbacks_as_failures()
{
    for (pair, current, dark, light, dark_source, light_source, reason) in [
        (
            None,
            None,
            "catppuccin-mocha",
            "catppuccin-latte",
            "brand_default",
            "brand_default",
            Some("no_current_theme"),
        ),
        (
            None,
            Some("PRIVATE_FIXTURE_CONTENT"),
            "catppuccin-mocha",
            "catppuccin-latte",
            "brand_default",
            "brand_default",
            Some("unknown_current_theme"),
        ),
        (
            None,
            Some("catppuccin-mocha"),
            "catppuccin-mocha",
            "catppuccin-latte",
            "current_theme",
            "catalog_pair",
            None,
        ),
        (
            None,
            Some("catppuccin-latte"),
            "catppuccin-mocha",
            "catppuccin-latte",
            "catalog_pair",
            "current_theme",
            None,
        ),
        (
            None,
            Some("nord"),
            "nord",
            "nord",
            "current_theme",
            "catalog_pair",
            None,
        ),
        (
            Some("dark_theme='gruvbox-dark'\nlight_theme='nord'\n"),
            Some("PRIVATE_FIXTURE_CONTENT"),
            "gruvbox-dark",
            "nord",
            "configured",
            "configured",
            None,
        ),
    ] {
        let td = tempfile::tempdir().unwrap();
        let env = fixture(td.path());
        if let Some(pair) = pair {
            write(&env.managed_file("auto.toml"), pair, 0o640);
        }
        if let Some(current) = current {
            write(&env.managed_file("current"), current, 0o600);
        }
        let report = compare(td.path(), false);
        for (appearance, id, source) in
            [("dark", dark, dark_source), ("light", light, light_source)]
        {
            let entry = &report["resolution"][appearance];
            assert_eq!(entry["status"], "resolved");
            assert_eq!(entry["theme_id"], id);
            assert_eq!(entry["source"], source);
            assert_eq!(entry["fallback_reason"].as_str(), reason);
        }
        if light == "nord" {
            assert_eq!(report["resolution"]["light"]["theme_appearance"], "dark");
        }
    }
}

#[test]
fn auto_theme_doctor_resolution_bounds_unsafe_inputs_and_keeps_independent_choices_under_writers() {
    for name in ["auto.toml", "current"] {
        for kind in [
            "unknown",
            "type",
            "syntax",
            "utf8",
            "large",
            "link",
            "fifo",
            "directory",
        ] {
            // Syntax and type are document-specific; current is a literal ID.
            if name == "current" && matches!(kind, "type" | "syntax" | "unknown") {
                continue;
            }
            let td = tempfile::tempdir().unwrap();
            let env = fixture(td.path());
            let _guard = ConfigWriteGuard::acquire(&env).unwrap();
            write(
                &env.slate_cache_dir().join("preview-session.json"),
                "PRIVATE_FIXTURE_CONTENT",
                0o600,
            );
            if name == "current" {
                write(&env.managed_file("auto.toml"), "dark_theme='nord'\n", 0o640);
            }
            let path = env.managed_file(name);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            match kind {
                "unknown" => write(
                    &path,
                    "dark_theme='nord'\nlight_theme='PRIVATE_FIXTURE_CONTENT'\n",
                    0o600,
                ),
                "type" => write(
                    &path,
                    "dark_theme='nord'\nlight_theme=['PRIVATE_FIXTURE_CONTENT']\n",
                    0o600,
                ),
                "syntax" => write(&path, "PRIVATE_FIXTURE_CONTENT = [", 0o600),
                "utf8" => write(&path, [0xff, 0xfe], 0o600),
                "large" => write(
                    &path,
                    vec![b'x'; if name == "auto.toml" { 262145 } else { 4097 }],
                    0o600,
                ),
                "link" => {
                    let target = td.path().join("target");
                    write(&target, "PRIVATE_FIXTURE_CONTENT", 0o600);
                    symlink(target, &path).unwrap();
                }
                "fifo" => {
                    let fifo = std::ffi::CString::new(path.as_os_str().as_encoded_bytes()).unwrap();
                    assert_eq!(unsafe { libc::mkfifo(fifo.as_ptr(), 0o600) }, 0);
                }
                "directory" => fs::create_dir(&path).unwrap(),
                _ => unreachable!(),
            }
            let report = compare(td.path(), true);
            assert_eq!(report["resolution"]["light"]["status"], "error");
            assert!(report["resolution"]["light"]["theme_id"].is_null());
            let independent = name == "current" || kind == "unknown";
            assert_eq!(
                report["resolution"]["dark"]["status"],
                if independent { "resolved" } else { "error" }
            );
            if independent {
                assert_eq!(report["resolution"]["dark"]["theme_id"], "nord");
            }
            if name == "current" {
                // Neither explicit choice consumes current, even when that path
                // is unsafe. This still does not prove application write-readiness.
                write(
                    &env.managed_file("auto.toml"),
                    "dark_theme='nord'\nlight_theme='catppuccin-latte'\n",
                    0o640,
                );
                let report = compare(td.path(), false);
                assert_eq!(
                    report["resolution"]["light"]["theme_id"],
                    "catppuccin-latte"
                );
            }
        }
    }
}
