use super::*;
use slate_cli::adapter::FastfetchAdapter;

fn seed() -> (tempfile::TempDir, SlateEnv) {
    let (temp, env) = fixture();
    let binary = env.user_local_bin().join("fastfetch");
    write(
        &binary,
        "#!/bin/sh\nprintf unexpected > \"$HOME/UNEXPECTED_PROCESS\"\nexit 91\n",
    );
    fs::set_permissions(binary, fs::Permissions::from_mode(0o755)).unwrap();
    FastfetchAdapter
        .apply_theme_with_env(ThemeRegistry::new().unwrap().get("nord").unwrap(), &env)
        .unwrap();
    (temp, env)
}

#[test]
fn fastfetch_doctor_compares_preset_without_execution_or_writes() {
    let (_temp, env) = seed();
    let before = tree_snapshot::tree(env.home());
    let report = diagnose(&env, "fastfetch");
    assert_eq!(check(&report, "preset_match")["status"], "ok");
    assert_eq!(check(&report, "activation_unverified")["status"], "info");
    assert_eq!(tree_snapshot::tree(env.home()), before);
    let original = fs::read_to_string(FastfetchAdapter::theme_path(&env)).unwrap();
    for content in [
        format!("// personal comment\n{original}\n"),
        serde_json::to_string(&serde_json::from_str::<serde_json::Value>(&original).unwrap())
            .unwrap(),
    ] {
        write(&FastfetchAdapter::theme_path(&env), content);
        let before = tree_snapshot::tree(env.home());
        let report = diagnose(&env, "fastfetch");
        assert_eq!(check(&report, "preset_match")["status"], "ok");
        assert!(check(&report, "preset_match")["message"]
            .as_str()
            .unwrap()
            .contains("apart from comments"));
        assert_eq!(tree_snapshot::tree(env.home()), before);
    }
    write(
        &FastfetchAdapter::theme_path(&env),
        original.replace("38;2;", "38;5;"),
    );
    assert_eq!(
        check(&diagnose(&env, "fastfetch"), "preset_match")["status"],
        "warning"
    );
    write(
        &FastfetchAdapter::theme_path(&env),
        "PRIVATE_PRESET_CONTENT",
    );
    let before = tree_snapshot::tree(env.home());
    let report = diagnose(&env, "fastfetch");
    assert_eq!(check(&report, "preset_match")["status"], "warning");
    assert!(!report.to_string().contains("PRIVATE_PRESET_CONTENT"));
    assert_eq!(tree_snapshot::tree(env.home()), before);
    write(&env.managed_file("current"), "unknown-theme");
    let report = diagnose(&env, "fastfetch");
    assert_eq!(check(&report, "saved_theme")["status"], "warning");
    assert!(!report["checks"]
        .as_array()
        .unwrap()
        .iter()
        .any(|check| check["code"] == "preset_match"));
}

#[test]
fn fastfetch_doctor_bounds_reads_and_rejects_links_without_content_leaks() {
    for case in ["missing", "utf8", "symlink", "oversize"] {
        let (_temp, env) = seed();
        let path = FastfetchAdapter::theme_path(&env);
        fs::remove_file(&path).unwrap();
        match case {
            "utf8" => write(&path, b"PRIVATE_PRESET\xff"),
            "symlink" => {
                let other = env.home().join("private-source");
                write(&other, "PRIVATE_PRESET");
                symlink(other, &path).unwrap();
            }
            "oversize" => write(&path, vec![b'x'; 8 * 1024 * 1024 + 1]),
            _ => {}
        }
        let before = tree_snapshot::tree(env.home());
        let report = diagnose(&env, "fastfetch");
        assert_eq!(
            check(&report, "preset_file")["status"],
            if case == "missing" { "info" } else { "error" }
        );
        assert!(!report.to_string().contains("PRIVATE_PRESET"));
        assert_eq!(tree_snapshot::tree(env.home()), before);
    }
}
