//! Public lookup APIs in a private PATH. No candidate executable is ever run.
use slate_cli::{
    detection::{self, ToolEvidence},
    env::SlateEnv,
};
use std::{fs, os::unix::fs::PermissionsExt, path::Path, time::Duration};

#[path = "support/tree.rs"]
mod snapshot;

fn write(path: &Path, mode: u32) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, "#!/bin/sh\nexit 99\n").unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(mode)).unwrap();
}

#[test]
#[ignore = "run by the parent with a private PATH and a deadline"]
fn executable_lookup_public_child() {
    let env = SlateEnv::from_process().unwrap();
    let home = env.home();
    let name = "slate-command-lookup-fixture";
    let first = home.join("first").join(name);
    let second = home.join("second").join(name);
    let local = home.join(".local/bin").join(name);
    write(&first, 0o644);
    write(&second, 0o755);
    write(&local, 0o644);
    let before = snapshot::tree(home);
    let presence = detection::detect_tool_presence_with_env(name, &env);
    assert!(presence.installed && presence.in_path);
    assert_eq!(
        presence.evidence,
        Some(ToolEvidence::Executable(second.clone()))
    );
    assert_eq!(
        detection::command_path_with_env(name, &env),
        Some(second.clone())
    );
    assert_eq!(detection::command_path(name), Some(second.clone()));
    assert_eq!(snapshot::tree(home), before);

    fs::set_permissions(&second, fs::Permissions::from_mode(0o644)).unwrap();
    let before = snapshot::tree(home);
    assert!(!detection::detect_tool_presence_with_env(name, &env).installed);
    assert!(detection::command_path_with_env(name, &env).is_none());
    assert_eq!(snapshot::tree(home), before);

    fs::set_permissions(&local, fs::Permissions::from_mode(0o755)).unwrap();
    let before = snapshot::tree(home);
    let presence = detection::detect_tool_presence_with_env(name, &env);
    assert!(presence.installed && !presence.in_path);
    assert_eq!(
        presence.evidence,
        Some(ToolEvidence::Executable(local.clone()))
    );
    assert_eq!(detection::command_path_with_env(name, &env), Some(local));
    assert_eq!(snapshot::tree(home), before);

    write(&home.join("first/bat"), 0o644);
    let batcat = home.join("second/batcat");
    write(&batcat, 0o755);
    let before = snapshot::tree(home);
    let presence = detection::detect_tool_presence_with_env("bat", &env);
    assert!(presence.installed && presence.in_path);
    assert_eq!(presence.evidence, Some(ToolEvidence::Executable(batcat)));
    assert_eq!(snapshot::tree(home), before);
}

#[test]
fn executable_lookup_public_paths_fallbacks_and_aliases_remain_read_only() {
    let home = tempfile::tempdir().unwrap();
    let path =
        std::env::join_paths([home.path().join("first"), home.path().join("second")]).unwrap();
    assert_cmd::Command::new(std::env::current_exe().unwrap())
        .env_clear()
        .env("HOME", home.path())
        .env("SLATE_HOME", home.path())
        .env("PATH", path)
        .current_dir(home.path())
        .args([
            "--exact",
            "executable_lookup_public_child",
            "--ignored",
            "--nocapture",
        ])
        .timeout(Duration::from_secs(7))
        .assert()
        .success();
}
