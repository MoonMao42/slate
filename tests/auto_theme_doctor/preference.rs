use super::*;
use serde_json::Value;

fn compare(home: &Path, expected: Option<bool>) {
    let before = tree(home);
    let report = inspect(home);
    assert_eq!(
        report["auto_theme_enabled"],
        serde_json::to_value(expected).unwrap()
    );
    let output = command(home)
        .args(["config", "get", "auto-theme", "--json"])
        .assert()
        .success()
        .get_output()
        .clone();
    assert!(output.stderr.is_empty());
    assert!(!String::from_utf8_lossy(&output.stdout).contains("PRIVATE_FIXTURE_CONTENT"));
    let config: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(config["settings"][0]["value"], report["auto_theme_enabled"]);
    assert_eq!(
        config["settings"][0]["status"],
        if expected.is_some() { "ok" } else { "error" }
    );
    assert_eq!(
        report["issues"].as_array().unwrap().iter().any(|issue| {
            issue
                .as_str()
                .unwrap()
                .contains("saved auto-theme preference cannot be read")
        }),
        expected.is_none()
    );
    let output = command(home)
        .args(["doctor", "auto-theme"])
        .assert()
        .success()
        .get_output()
        .clone();
    assert!(output.stderr.is_empty());
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(text.contains(match expected {
        Some(true) => "Enabled: yes\n",
        Some(false) => "Enabled: no\n",
        None => "Enabled: unknown\n",
    }));
    assert!(!text.contains("PRIVATE_FIXTURE_CONTENT"));
    assert_eq!(tree(home), before);
}

#[test]
fn auto_theme_doctor_preference_rejects_dangling_parents_and_isolated_escapes_without_false_defaults(
) {
    for kind in [
        "dangling-root",
        "dangling-slate",
        "external-present",
        "external-absent",
        "safe-alias",
        "safe-alias-absent",
        "file-parent",
        "final-link",
        "dangling-final",
    ] {
        let td = tempfile::tempdir().unwrap();
        let home = td.path().join("home");
        fs::create_dir(&home).unwrap();
        let env = SlateEnv::with_home(home.clone());
        match kind {
            "dangling-root" => symlink(home.join("missing"), home.join(".config")).unwrap(),
            "dangling-slate" => {
                fs::create_dir(home.join(".config")).unwrap();
                symlink(home.join("missing"), env.config_dir()).unwrap();
            }
            "external-present" | "external-absent" | "safe-alias" | "safe-alias-absent" => {
                let target = if kind.starts_with("safe-alias") {
                    home.join("settings")
                } else {
                    td.path().join("external")
                };
                fs::create_dir(&target).unwrap();
                if !kind.ends_with("absent") {
                    write(
                        &target.join("slate/config.toml"),
                        "[auto_theme]\nenabled=true\n# PRIVATE_FIXTURE_CONTENT\n",
                        0o640,
                    );
                }
                symlink(&target, home.join(".config")).unwrap();
            }
            "file-parent" => write(&home.join(".config"), "PRIVATE_FIXTURE_CONTENT", 0o600),
            "final-link" | "dangling-final" => {
                fs::create_dir_all(env.config_dir()).unwrap();
                let target = home.join("target");
                if kind == "final-link" {
                    write(&target, "auto_theme = { enabled = true }\n", 0o640);
                }
                symlink(target, env.managed_file("config.toml")).unwrap();
            }
            _ => unreachable!(),
        }
        let before = tree(td.path());
        compare(
            &home,
            match kind {
                "safe-alias" => Some(true),
                "safe-alias-absent" => Some(false),
                _ => None,
            },
        );
        assert_eq!(tree(td.path()), before, "including external fixtures");
    }
}

#[test]
fn auto_theme_doctor_preference_matches_shared_defaults_types_and_bounds_without_writes() {
    for (bytes, expected) in [
        (None, Some(false)),
        (Some(Vec::new()), Some(false)),
        (
            Some(b"# PRIVATE_FIXTURE_CONTENT\n[auto_theme]\n".to_vec()),
            Some(false),
        ),
        (Some(b"auto_theme.enabled = true\n".to_vec()), Some(true)),
        (
            Some(b"auto_theme = { enabled = false }\n".to_vec()),
            Some(false),
        ),
        (
            Some(
                b"[auto_theme]\nenabled=true\n[preferences]\nsound='PRIVATE_FIXTURE_CONTENT'\n"
                    .to_vec(),
            ),
            Some(true),
        ),
        (
            Some(b"auto_theme = 'PRIVATE_FIXTURE_CONTENT'\n".to_vec()),
            None,
        ),
        (
            Some(b"[auto_theme]\nenabled='PRIVATE_FIXTURE_CONTENT'\n".to_vec()),
            None,
        ),
        (Some(b"PRIVATE_FIXTURE_CONTENT = [".to_vec()), None),
        (Some(vec![0xff, 0xfe]), None),
        (Some(vec![b'#'; 262144]), Some(false)),
        (Some(vec![b'#'; 262145]), None),
    ] {
        let td = tempfile::tempdir().unwrap();
        let env = SlateEnv::with_home(td.path().to_owned());
        if let Some(bytes) = bytes {
            write(&env.managed_file("config.toml"), bytes, 0o640);
        }
        compare(td.path(), expected);
    }
}
