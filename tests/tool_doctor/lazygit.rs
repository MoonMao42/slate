use super::*;
use slate_cli::adapter::LazygitAdapter;

fn seed() -> (tempfile::TempDir, SlateEnv) {
    let (temp, env) = fixture();
    write(
        &env.user_local_bin().join("lazygit"),
        "#!/bin/sh\nprintf unexpected > \"$HOME/UNEXPECTED_PROCESS\"\nexit 91\n",
    );
    fs::set_permissions(
        env.user_local_bin().join("lazygit"),
        fs::Permissions::from_mode(0o755),
    )
    .unwrap();
    LazygitAdapter
        .apply_theme_with_env(ThemeRegistry::new().unwrap().get("nord").unwrap(), &env)
        .unwrap();
    (temp, env)
}

#[test]
fn lazygit_doctor_separates_palette_from_missing_legacy_and_custom_selection() {
    let (_temp, env) = seed();
    let managed = LazygitAdapter::theme_path(&env);
    let personal = env.xdg_config_home().join("lazygit/config.yml");
    write(&personal, "PRIVATE_CONTENT: not evaluated as YAML\n");
    let before = tree_snapshot::tree(env.home());
    let report = json(&mut command(&env, false), "lazygit");
    assert_eq!(check(&report, "palette_match")["status"], "ok");
    assert_eq!(check(&report, "environment_selection")["status"], "warning");
    for (selection, status, missing) in [
        (managed.display().to_string(), "info", false),
        (
            format!("{},{}", managed.display(), personal.display()),
            "info",
            false,
        ),
        (
            format!("{}:{}", managed.display(), personal.display()),
            "error",
            false,
        ),
        (personal.display().to_string(), "warning", false),
        (
            format!(
                "{},{}",
                managed.display(),
                env.home().join("missing.yml").display()
            ),
            "info",
            true,
        ),
        (
            format!("{},relative.yml,", managed.display()),
            "info",
            false,
        ),
    ] {
        let report = json(
            command(&env, false).env("LG_CONFIG_FILE", selection),
            "lazygit",
        );
        assert_eq!(check(&report, "environment_selection")["status"], status);
        if status == "error" {
            let suggestion = check(&report, "environment_selection")["suggestion"]
                .as_str()
                .unwrap();
            assert!(suggestion.starts_with("First open a fresh terminal"));
            assert!(suggestion.contains("saved startup files may already be updated"));
            assert!(suggestion.contains("If the legacy value persists"));
        }
        assert_eq!(
            report["checks"]
                .as_array()
                .unwrap()
                .iter()
                .any(|c| c["code"] == "selected_file" && c["status"] == "error"),
            missing
        );
        assert!(!serde_json::to_string(&report)
            .unwrap()
            .contains("PRIVATE_CONTENT"));
        assert_eq!(tree_snapshot::tree(env.home()), before);
    }
    let colon = env.home().join("colon:filename.yml");
    write(&colon, "PRIVATE_CONTENT");
    let report = json(
        command(&env, false).env("LG_CONFIG_FILE", &colon),
        "lazygit",
    );
    assert_eq!(check(&report, "selected_file")["status"], "info");
    assert_eq!(
        check(&report, "selected_file")["path"],
        colon.to_str().unwrap()
    );
}

#[test]
fn lazygit_doctor_remains_bounded_read_only_and_available_without_valid_theme() {
    let (_temp, env) = seed();
    write(&env.managed_file("current"), "PRIVATE_UNKNOWN");
    write(
        &env.slate_cache_dir().join("preview-session.json"),
        "PRIVATE_RECOVERY",
    );
    write(&LazygitAdapter::theme_path(&env), "PRIVATE_CONTENT");
    let before = tree_snapshot::tree(env.home());
    for selection in ["/tmp/file.yml,".repeat(33), "x".repeat(16 * 1024 + 1)] {
        let report = json(
            command(&env, false).env("LG_CONFIG_FILE", selection),
            "lazygit",
        );
        assert_eq!(check(&report, "environment_selection")["status"], "warning");
        absent(&report, "selected_file");
        absent(&report, "palette_match");
        assert!(!serde_json::to_string(&report).unwrap().contains("PRIVATE_"));
    }
    let isolated = json(
        command(&env, true).env("LG_CONFIG_FILE", "/outside/PRIVATE_OVERRIDE"),
        "lazygit",
    );
    assert_eq!(check(&isolated, "isolated_profile")["status"], "info");
    assert_eq!(
        check(&isolated, "environment_selection")["status"],
        "warning"
    );
    assert_eq!(tree_snapshot::tree(env.home()), before);
    let info = command(&env, true)
        .args(["tools", "info", "lazygit", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    assert!(String::from_utf8(info)
        .unwrap()
        .contains("slate doctor lazygit"));
    assert_eq!(tree_snapshot::tree(env.home()), before);
}

#[test]
fn lazygit_doctor_never_reads_fifo_or_linked_selected_files() {
    let (_temp, env) = seed();
    let fifo = env.home().join("fifo.yml");
    let name = std::ffi::CString::new(fifo.to_str().unwrap()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
    let linked = env.home().join("linked.yml");
    symlink(&fifo, &linked).unwrap();
    for path in [&fifo, &linked] {
        let report = json(command(&env, false).env("LG_CONFIG_FILE", path), "lazygit");
        assert_eq!(check(&report, "selected_file")["status"], "warning");
    }
    let managed = LazygitAdapter::theme_path(&env);
    fs::remove_file(&managed).unwrap();
    symlink(&fifo, &managed).unwrap();
    let report = json(&mut command(&env, false), "lazygit");
    assert_eq!(check(&report, "fragment_file")["status"], "error");
    absent(&report, "palette_match");
    assert!(fs::symlink_metadata(&managed)
        .unwrap()
        .file_type()
        .is_symlink());
    assert!(!env.home().join("UNEXPECTED_PROCESS").exists());
}
