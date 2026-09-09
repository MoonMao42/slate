use assert_cmd::Command;
use slate_cli::config::{
    begin_restore_point_baseline_with_env, execute_restore_with_env, get_restore_point_with_env,
    preview_restore_with_env, ConfigWriteGuard, RestoreAction,
};
use slate_cli::env::SlateEnv;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

#[path = "restore_preview/output.rs"]
mod output;

fn tree(root: &Path) -> BTreeMap<PathBuf, Option<Vec<u8>>> {
    fn visit(root: &Path, dir: &Path, out: &mut BTreeMap<PathBuf, Option<Vec<u8>>>) {
        for entry in fs::read_dir(dir).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            if entry.file_type().unwrap().is_dir() {
                out.insert(path.strip_prefix(root).unwrap().to_owned(), None);
                visit(root, &path, out);
            } else {
                out.insert(
                    path.strip_prefix(root).unwrap().to_owned(),
                    Some(fs::read(path).unwrap()),
                );
            }
        }
    }
    let mut result = BTreeMap::new();
    visit(root, root, &mut result);
    result
}

fn cli(home: &TempDir) -> Command {
    let mut command = Command::cargo_bin("slate").unwrap();
    command.env("SLATE_HOME", home.path());
    command
}

#[test]
fn restore_preview_and_list_do_not_create_files_on_empty_home() {
    let td = TempDir::new().unwrap();
    for args in [
        vec!["restore", "--list"],
        vec!["restore", "missing", "--dry-run", "--json"],
    ] {
        let output = cli(&td).args(&args).output().unwrap();
        assert_eq!(output.status.success(), args.contains(&"--list"));
        assert!(tree(td.path()).is_empty());
    }
    for args in [
        vec!["restore", "--dry-run"],
        vec!["restore", "some-id", "--json"],
        vec!["restore", "some-id", "--list"],
        vec!["restore", "--list", "--delete", "some-id"],
    ] {
        assert_eq!(cli(&td).args(args).output().unwrap().status.code(), Some(2));
        assert!(tree(td.path()).is_empty());
    }
}

#[test]
fn restore_preview_reports_byte_changes_without_mutation() {
    let td = TempDir::new().unwrap();
    let env = SlateEnv::with_home(td.path().to_owned());
    fs::write(env.zshrc_path(), b"# private fixture\xff\n").unwrap();
    fs::write(env.bashrc_path(), b"original bash\n").unwrap();
    fs::write(env.home().join(".gitconfig"), "[user]\nname = unchanged\n").unwrap();
    let point = begin_restore_point_baseline_with_env(&env).unwrap();
    fs::write(env.zshrc_path(), "changed shell\n").unwrap();
    fs::remove_file(env.bashrc_path()).unwrap();
    fs::write(env.bash_profile_path(), "added after snapshot\n").unwrap();
    let before = tree(td.path());
    let plan = preview_restore_with_env(&env, &point.id).unwrap();
    for (path, action) in [
        (env.zshrc_path(), RestoreAction::Replace),
        (env.bashrc_path(), RestoreAction::Create),
        (env.bash_profile_path(), RestoreAction::Remove),
        (env.home().join(".gitconfig"), RestoreAction::Unchanged),
    ] {
        assert_eq!(
            plan.changes
                .iter()
                .find(|c| c.original_path == path)
                .unwrap()
                .action,
            action
        );
    }
    assert_eq!(plan.changed_count(), 3);
    let output = cli(&td)
        .args(["restore", &point.id, "--dry-run", "--json"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["restore_point_id"], point.id);
    assert_eq!(json["may_regenerate_theme_files"], false);
    assert!(!String::from_utf8_lossy(&output.stdout).contains("private fixture"));
    assert_eq!(tree(td.path()), before);
}

#[test]
fn restore_preflight_rejects_a_late_directory_before_changing_any_file() {
    let td = TempDir::new().unwrap();
    let env = SlateEnv::with_home(td.path().to_owned());
    // Writers retain one empty lock inode; seed it before the no-mutation
    // comparison instead of hiding it (or any other file) from the tree check.
    drop(ConfigWriteGuard::acquire(&env).unwrap());
    fs::write(env.zshrc_path(), "old\n").unwrap();
    fs::write(env.bashrc_path(), "old\n").unwrap();
    let point = begin_restore_point_baseline_with_env(&env).unwrap();
    fs::write(env.zshrc_path(), "new\n").unwrap();
    fs::remove_file(env.bashrc_path()).unwrap();
    fs::create_dir(env.bashrc_path()).unwrap();
    let before = tree(td.path());
    let plan = preview_restore_with_env(&env, &point.id).unwrap();
    assert_eq!(plan.blocked_count(), 1);
    assert!(execute_restore_with_env(&env, &point.id).is_err());
    let output = cli(&td)
        .args(["restore", &point.id, "--dry-run", "--json"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(json["changes"]
        .as_array()
        .unwrap()
        .iter()
        .any(|c| c["action"] == "blocked"));
    assert_eq!(tree(td.path()), before);

    fs::remove_dir(env.bashrc_path()).unwrap();
    let link_target = td.path().join("user-shell-file");
    fs::write(&link_target, "# preserve this file and its link\n").unwrap();
    std::os::unix::fs::symlink(&link_target, env.bashrc_path()).unwrap();
    let before = tree(td.path());
    assert_eq!(
        preview_restore_with_env(&env, &point.id)
            .unwrap()
            .blocked_count(),
        1
    );
    assert!(execute_restore_with_env(&env, &point.id).is_err());
    assert_eq!(fs::read_link(env.bashrc_path()).unwrap(), link_target);
    assert_eq!(tree(td.path()), before);
}

#[test]
fn restore_rejects_damaged_backups_before_changing_targets() {
    let td = TempDir::new().unwrap();
    let env = SlateEnv::with_home(td.path().to_owned());
    drop(ConfigWriteGuard::acquire(&env).unwrap());
    fs::write(env.zshrc_path(), "old zsh\n").unwrap();
    fs::write(env.bashrc_path(), "old bash\n").unwrap();
    let point = begin_restore_point_baseline_with_env(&env).unwrap();
    fs::write(env.zshrc_path(), "new zsh\n").unwrap();
    let last = point
        .entries
        .iter()
        .find(|entry| entry.tool_key == "bashrc")
        .unwrap();
    fs::remove_file(last.backup_path.as_ref().unwrap()).unwrap();
    let before = tree(td.path());
    assert!(preview_restore_with_env(&env, &point.id).is_err());
    assert!(execute_restore_with_env(&env, &point.id).is_err());
    assert_eq!(tree(td.path()), before);
}

#[test]
fn restore_undo_covers_original_profile_even_after_config_root_changes() {
    let td = TempDir::new().unwrap();
    let old_env = SlateEnv::with_home(td.path().to_owned());
    let init = old_env.nvim_config_dir().join("init.lua");
    fs::create_dir_all(init.parent().unwrap()).unwrap();
    fs::write(&init, "-- original profile\n").unwrap();
    let point = begin_restore_point_baseline_with_env(&old_env).unwrap();
    fs::write(&init, "-- current edits\n").unwrap();
    fs::write(old_env.bashrc_path(), "# added after baseline\n").unwrap();
    let new_env = SlateEnv::from_vars(|key| match key {
        "HOME" => Some(td.path().as_os_str().to_owned()),
        "XDG_CONFIG_HOME" => Some(td.path().join("new config").into_os_string()),
        "NVIM_APPNAME" => Some("work".into()),
        _ => None,
    })
    .unwrap();
    let receipt = execute_restore_with_env(&new_env, &point.id).unwrap();
    assert!(receipt.is_fully_successful());
    assert_eq!(fs::read_to_string(&init).unwrap(), "-- original profile\n");
    assert!(!old_env.bashrc_path().exists());
    let undo = get_restore_point_with_env(&new_env, &receipt.pre_restore_point_id).unwrap();
    assert!(undo.entries.iter().any(|entry| entry.original_path == init));
    assert!(execute_restore_with_env(&new_env, &undo.id)
        .unwrap()
        .is_fully_successful());
    assert_eq!(fs::read_to_string(&init).unwrap(), "-- current edits\n");
    assert_eq!(
        fs::read_to_string(old_env.bashrc_path()).unwrap(),
        "# added after baseline\n"
    );
    assert!(!new_env.nvim_config_dir().exists());
}

#[test]
fn failed_snapshot_does_not_leave_a_partial_restore_point() {
    let td = TempDir::new().unwrap();
    let env = SlateEnv::with_home(td.path().to_owned());
    fs::write(env.zshrc_path(), "# first target can be backed up\n").unwrap();
    fs::create_dir(env.bashrc_path()).unwrap();
    assert!(begin_restore_point_baseline_with_env(&env).is_err());
    assert_eq!(
        fs::read_dir(env.slate_cache_dir().join("backups"))
            .unwrap()
            .count(),
        0
    );
}

#[test]
fn theme_application_stops_when_its_restore_point_cannot_be_saved() {
    for prior_theme in [None, Some("nord")] {
        let td = TempDir::new().unwrap();
        let env = SlateEnv::with_home(td.path().to_owned());
        drop(ConfigWriteGuard::acquire(&env).unwrap());
        let config = slate_cli::config::ConfigManager::with_env(&env).unwrap();
        if let Some(prior_theme) = prior_theme {
            config.set_current_theme(prior_theme).unwrap();
        }
        fs::write(env.zshrc_path(), "# original shell\n").unwrap();
        let blocked_target = env.managed_file("managed/shell/env.bash");
        fs::create_dir_all(&blocked_target).unwrap();
        let before = tree(td.path());
        let registry = slate_cli::theme::ThemeRegistry::new().unwrap();
        let result = slate_cli::cli::theme_apply::ThemeApplyCoordinator::new(&env)
            .apply(registry.get("catppuccin-mocha").unwrap());
        assert!(result.is_err());
        assert_eq!(tree(td.path()), before);
    }
}

#[test]
fn restore_rejects_ambiguous_or_unsafe_manifest_entries() {
    let td = TempDir::new().unwrap();
    let env = SlateEnv::with_home(td.path().to_owned());
    drop(ConfigWriteGuard::acquire(&env).unwrap());
    let point = begin_restore_point_baseline_with_env(&env).unwrap();
    let path = env
        .slate_cache_dir()
        .join("backups")
        .join(&point.id)
        .join("manifest.toml");
    let original: toml::Value = fs::read_to_string(&path).unwrap().parse().unwrap();
    for field in ["tool_key", "original_path", "unsafe_key", "unix_mode"] {
        let mut manifest = original.clone();
        let entries = manifest["entries"].as_array_mut().unwrap();
        if field == "unsafe_key" {
            entries[0]["tool_key"] = toml::Value::String("../escape".into());
        } else if field == "unix_mode" {
            entries[0]
                .as_table_mut()
                .unwrap()
                .insert("unix_mode".into(), toml::Value::Integer(0o4755));
        } else {
            entries[1][field] = entries[0][field].clone();
        }
        fs::write(&path, toml::to_string(&manifest).unwrap()).unwrap();
        let before = tree(td.path());
        assert!(preview_restore_with_env(&env, &point.id).is_err());
        assert!(execute_restore_with_env(&env, &point.id).is_err());
        assert_eq!(tree(td.path()), before);
    }
}
