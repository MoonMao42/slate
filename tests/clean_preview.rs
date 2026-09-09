//! Real clean previews and execution comparisons only in disposable profiles.
use slate_cli::config::{execute_restore_with_env, list_restore_points_with_env, ConfigWriteGuard};
use slate_cli::env::SlateEnv;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::os::fd::OwnedFd;
use std::os::unix::{
    fs::{symlink, MetadataExt, PermissionsExt},
    net::UnixStream,
};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

#[path = "support/redirected_output.rs"]
mod redirected_output;
#[path = "support/tree.rs"]
mod tree_snapshot;

fn command(home: &Path) -> assert_cmd::Command {
    let mut command = assert_cmd::Command::new(assert_cmd::cargo::cargo_bin!("slate"));
    command
        .env_clear()
        .env("HOME", home)
        .env("SLATE_HOME", home)
        .env("PATH", home.join("bin"))
        .env("NO_COLOR", "1")
        .timeout(Duration::from_secs(5));
    command
}

fn write(path: &Path, content: impl AsRef<[u8]>) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, content).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o640)).unwrap();
}

fn json_preview(home: &Path, success: bool) -> serde_json::Value {
    let output = command(home)
        .args(["clean", "--dry-run", "--json"])
        .assert()
        .code(if success { 0 } else { 1 })
        .get_output()
        .clone();
    assert!(!String::from_utf8_lossy(&output.stdout).contains("PRIVATE_CONTENT"));
    assert!(!String::from_utf8_lossy(&output.stderr).contains("PRIVATE_CONTENT"));
    serde_json::from_slice(&output.stdout).unwrap()
}

fn profile_files(env: &SlateEnv) -> BTreeMap<PathBuf, (u32, Vec<u8>)> {
    tree_snapshot::tree(env.home())
        .into_iter()
        .filter(|(path, _)| {
            !fs::symlink_metadata(path).unwrap().is_dir()
                && !path.starts_with(env.slate_cache_dir().join("backups"))
                && path != &env.slate_cache_dir().join("preview-session.lock")
        })
        .collect()
}

#[test]
fn bash_startup_clean_preview_and_restore_preserve_all_user_entries() {
    use slate_cli::{
        adapter::marker_block::{END, START},
        config::begin_restore_point_baseline,
    };
    let home = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(home.path().into());
    for path in [env.bash_login_path(), env.shell_profile_path()] {
        write(&path, "# PRIVATE_CONTENT user profile\n");
    }
    let original = profile_files(&env);
    let baseline = begin_restore_point_baseline(home.path()).unwrap();
    for (key, path) in [
        ("bash-login", env.bash_login_path()),
        ("shell-profile", env.shell_profile_path()),
    ] {
        let entry = baseline
            .entries
            .iter()
            .find(|entry| entry.tool_key == key)
            .unwrap();
        assert_eq!(entry.original_path, path);
        assert_eq!(
            fs::read(entry.backup_path.as_ref().unwrap()).unwrap(),
            fs::read(&path).unwrap()
        );
    }
    let paths = [
        env.bashrc_path(),
        env.bash_profile_path(),
        env.bash_login_path(),
        env.shell_profile_path(),
    ];
    for path in &paths {
        let user = fs::read_to_string(path).unwrap_or_default();
        write(
            path,
            format!("{user}{START}\n# private fixture loader\n{END}\n"),
        );
    }
    let integrated = profile_files(&env);
    let preview = json_preview(home.path(), true);
    assert_eq!(profile_files(&env), integrated);
    for path in &paths {
        assert!(preview["changes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry["path"] == path.to_str().unwrap() && entry["action"] == "rewrite"));
    }
    command(home.path())
        .args(["--quiet", "clean"])
        .assert()
        .success();
    for path in &paths {
        assert_eq!(
            fs::read(path).unwrap(),
            original
                .get(path)
                .map_or(&[][..], |(_, bytes)| bytes.as_slice())
        );
    }
    let cleaned = list_restore_points_with_env(&env)
        .unwrap()
        .into_iter()
        .find(|point| point.id != baseline.id)
        .unwrap();
    assert!(execute_restore_with_env(&env, &cleaned.id)
        .unwrap()
        .is_fully_successful());
    assert_eq!(profile_files(&env), integrated);
    assert!(execute_restore_with_env(&env, &baseline.id)
        .unwrap()
        .is_fully_successful());
    assert_eq!(profile_files(&env), original);
}

#[test]
fn ghostty_literal_clean_preview_preserves_embedded_user_paths_and_restores_owned_refs() {
    let home = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(home.path().to_owned());
    let config = env.xdg_config_home().join("ghostty/config.ghostty");
    let managed = env.managed_file("managed/ghostty");
    let user = format!("config-file = /outside # {}/font.conf\nconfig-file = '{}/theme.conf'\nconfig-file = {}/../user.conf\n", managed.display(), managed.display(), managed.display());
    let original = format!(
        "\u{feff}config-file = ?{}/theme.conf\n{user}",
        managed.display()
    );
    write(&config, original.as_bytes());
    let before = profile_files(&env);
    let preview = json_preview(home.path(), true);
    assert_eq!(profile_files(&env), before);
    assert!(preview["changes"]
        .as_array()
        .unwrap()
        .iter()
        .any(|entry| entry["path"] == config.to_str().unwrap() && entry["action"] == "rewrite"));
    command(home.path())
        .args(["--quiet", "clean"])
        .assert()
        .success();
    assert_eq!(
        fs::read_to_string(&config).unwrap(),
        format!("\u{feff}{user}")
    );
    let point = list_restore_points_with_env(&env).unwrap().remove(0);
    assert!(execute_restore_with_env(&env, &point.id)
        .unwrap()
        .is_fully_successful());
    assert_eq!(profile_files(&env), before);
}

#[test]
fn clean_preview_and_execution_preserve_user_imports_comments_and_noop_files() {
    for with_managed in [false, true] {
        let home = tempfile::tempdir().unwrap();
        let env = SlateEnv::with_home(home.path().to_owned());
        let kitty_path = env.xdg_config_home().join("kitty/kitty.conf");
        let alacritty_path = env.xdg_config_home().join("alacritty/alacritty.toml");
        let kitty_root = env.config_dir().join("managed/kitty");
        let alacritty_root = env.config_dir().join("managed/alacritty");
        let kitty_user = format!("# PRIVATE_CONTENT\r\ninclude {}-custom/theme.conf\r\nlisten_on unix:/tmp/kitty-slate-personal\r\n", kitty_root.display());
        let kitty_owned = format!("include {}/\r\n  \\theme.conf\r\n", kitty_root.display());
        let kitty_input = format!(
            "{kitty_user}{}",
            if with_managed { &kitty_owned } else { "" }
        );
        let array_entry = format!("'{}'", alacritty_root.join("colors.toml").display());
        let alacritty_user = format!("# PRIVATE_CONTENT\r\n[general]\r\nimport = [\r\n  'user.toml', # retained\r\n  '{}-custom/theme.toml',\r\n  42, # preserve application-invalid data too\r\n", alacritty_root.display());
        let alacritty_input = format!(
            "{alacritty_user}{}] # tail\r\n",
            if with_managed {
                format!("  {array_entry}, # keep this note,\r\n")
            } else {
                String::new()
            }
        );
        let alacritty_expected = alacritty_input.replace(&format!("{array_entry},"), "");
        write(&kitty_path, &kitty_input);
        write(&alacritty_path, &alacritty_input);
        let before = profile_files(&env);
        let tree_before = tree_snapshot::tree(home.path());
        let metadata_before: Vec<_> = [&kitty_path, &alacritty_path]
            .into_iter()
            .map(|path| {
                let metadata = fs::metadata(path).unwrap();
                (metadata.ino(), metadata.modified().unwrap())
            })
            .collect();
        let plan = json_preview(home.path(), true);
        assert_eq!(tree_snapshot::tree(home.path()), tree_before);
        for path in [&kitty_path, &alacritty_path] {
            let change = plan["changes"]
                .as_array()
                .unwrap()
                .iter()
                .find(|change| change["path"].as_str() == path.to_str())
                .unwrap();
            assert_eq!(
                change["action"],
                if with_managed { "rewrite" } else { "unchanged" }
            );
        }
        command(home.path())
            .args(["--quiet", "clean"])
            .assert()
            .success();
        assert_eq!(fs::read(&kitty_path).unwrap(), kitty_user.as_bytes());
        assert_eq!(
            fs::read(&alacritty_path).unwrap(),
            alacritty_expected.as_bytes()
        );
        for (index, path) in [&kitty_path, &alacritty_path].into_iter().enumerate() {
            let metadata = fs::metadata(path).unwrap();
            assert_eq!(metadata.permissions().mode() & 0o777, 0o640);
            if !with_managed {
                assert_eq!(
                    (metadata.ino(), metadata.modified().unwrap()),
                    metadata_before[index]
                );
            }
        }
        if !with_managed {
            assert_eq!(profile_files(&env), before);
        }
        let point = list_restore_points_with_env(&env).unwrap().remove(0);
        assert!(execute_restore_with_env(&env, &point.id)
            .unwrap()
            .is_fully_successful());
        assert_eq!(profile_files(&env), before);
    }
}

#[test]
fn clean_opencode_projection_preserves_jsonc_bytes_and_is_restorable() {
    for (input, expected, action) in [
        ("// PRIVATE_CONTENT\r\n{\"theme\"/*k*/:/*v*/\"system\", \"user\":{\"n\":1.2300e+02,},}\r\n",
         Some("// PRIVATE_CONTENT\r\n{/*k*//*v*/ \"user\":{\"n\":1.2300e+02,},}\r\n"), "rewrite"),
        ("// PRIVATE_CONTENT\n{\"theme\":\"system\"}", Some("// PRIVATE_CONTENT\n{}"), "rewrite"),
        ("{\"theme\":\"custom\",\"x\":1.00}", Some("{\"theme\":\"custom\",\"x\":1.00}"), "unchanged"),
        ("{\"theme\":\"system\"}", None, "remove"),
    ] {
        let home = tempfile::tempdir().unwrap();
        let env = SlateEnv::with_home(home.path().to_owned());
        let path = env.xdg_config_home().join("opencode/tui.jsonc");
        write(&path, input);
        let before = profile_files(&env);
        let tree_before = tree_snapshot::tree(home.path());
        let meta_before = fs::metadata(&path).unwrap();
        let plan = json_preview(home.path(), true);
        assert_eq!(tree_snapshot::tree(home.path()), tree_before);
        let change = plan["changes"].as_array().unwrap().iter().find(|c| c["path"].as_str() == path.to_str()).unwrap();
        assert_eq!(change["action"], action);
        command(home.path()).args(["--quiet", "clean"]).assert().success();
        assert_eq!(fs::read_to_string(&path).ok().as_deref(), expected);
        if expected.is_some() { assert_eq!(fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o640); }
        if action == "unchanged" {
            let meta = fs::metadata(&path).unwrap();
            assert_eq!((meta.ino(), meta.modified().unwrap()), (meta_before.ino(), meta_before.modified().unwrap()));
        }
        let point = list_restore_points_with_env(&env).unwrap().remove(0);
        assert!(execute_restore_with_env(&env, &point.id).unwrap().is_fully_successful());
        assert_eq!(profile_files(&env), before);
    }
}

#[test]
fn clean_preview_is_read_only_on_empty_locked_and_pending_profiles() {
    use assert_cmd::assert::OutputAssertExt;
    let home = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(home.path().to_owned());
    let before = tree_snapshot::tree(home.path());
    let plan = json_preview(home.path(), true);
    assert_eq!(plan["schema_version"], 1);
    assert_eq!(plan["summary"]["remove"], 0);
    assert_eq!(plan["summary"]["rewrite"], 0);
    assert_eq!(plan["scan_complete"], true);
    assert_eq!(plan["watcher_action"], "leave_untouched");
    assert_eq!(plan["terminal_reload"], "skip");
    assert_eq!(plan["snapshot_required"], true);
    assert_eq!(plan["snapshot_write_checked"], false);
    assert_eq!(plan["target_writes_checked"], false);
    assert_eq!(plan["writer_and_recovery_checked"], false);
    assert_eq!(plan["uninstalls_tools"], false);
    command(home.path())
        .args(["clean", "--dry-run"])
        .assert()
        .success()
        .stdout(predicates::str::contains("Preview only"));
    assert_eq!(tree_snapshot::tree(home.path()), before);

    write(
        &env.managed_file("config.toml"),
        b"PRIVATE_CONTENT_BAD_PREFERENCE\xff",
    );
    let guard = ConfigWriteGuard::acquire(&env).unwrap();
    let pending = env.slate_cache_dir().join("preview-session.json");
    let name = std::ffi::CString::new(pending.as_os_str().as_encoded_bytes()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
    let before = tree_snapshot::tree(home.path());
    let plan = json_preview(home.path(), true);
    assert_eq!(plan["summary"]["remove"], 1);
    command(home.path())
        .args(["--auto", "clean", "--dry-run"])
        .assert()
        .success();
    assert_eq!(tree_snapshot::tree(home.path()), before);
    drop(guard);

    for args in [
        vec!["clean", "--json"],
        vec!["clean", "--dry-run", "unexpected"],
    ] {
        command(home.path()).args(args).assert().code(2);
    }
    let (consumer, producer) = UnixStream::pair().unwrap();
    drop(consumer);
    let mut pipe = std::process::Command::new(assert_cmd::cargo::cargo_bin!("slate"));
    pipe.env_clear()
        .env("HOME", home.path())
        .env("SLATE_HOME", home.path())
        .env("PATH", home.path().join("bin"))
        .args(["clean", "--dry-run", "--json"])
        .stdout(Stdio::from(OwnedFd::from(producer)))
        .stderr(Stdio::piped());
    redirected_output::run(&mut pipe)
        .assert()
        .success()
        .stderr("");
    assert_eq!(tree_snapshot::tree(home.path()), before);
}

#[test]
fn clean_preview_actions_match_real_cleanup_and_its_recovery_contract() {
    let home = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(home.path().to_owned());
    write(
        &outside.path().join("user-config"),
        "PRIVATE_CONTENT_OUTSIDE\n",
    );
    fs::create_dir_all(env.config_dir().join("user")).unwrap();
    symlink(
        outside.path().join("user-config"),
        env.config_dir().join("user/linked"),
    )
    .unwrap();
    let outside_before = tree_snapshot::tree(outside.path());
    let marker = format!(
        "# user before\n{}\nPRIVATE_CONTENT_MANAGED\n{}\n# user after\n",
        slate_cli::adapter::marker_block::START,
        slate_cli::adapter::marker_block::END
    );
    for path in [env.zshrc_path(), env.home().join(".gitconfig")]
        .into_iter()
        .chain(env.tmux_config_candidates())
    {
        write(&path, &marker);
    }
    write(&env.bashrc_path(), "# PRIVATE_CONTENT_UNCHANGED\n");
    write(&env.fish_loader_path(), "# loader\n");
    write(&env.managed_file("current"), "nord\n");
    write(
        &env.managed_file("config.toml"),
        "PRIVATE_CONTENT_INVALID_PREFERENCE",
    );
    write(
        &env.managed_file("managed/ghostty/theme.conf"),
        b"PRIVATE_CONTENT_BINARY\xff\n",
    );
    fs::create_dir_all(env.managed_file("managed/empty")).unwrap();
    write(
        &env.nvim_config_dir().join("lua/slate/init.lua"),
        "-- loader\n",
    );
    write(
        &env.nvim_config_dir().join("colors/slate-nord.lua"),
        "-- shim\n",
    );
    write(
        &env.nvim_config_dir().join("colors/user.lua"),
        "-- private user theme\n",
    );
    write(
        &env.nvim_config_dir().join("init.lua"),
        format!(
            "-- {}\nload_slate\n-- {}\nvim.opt.number = true\n",
            slate_cli::adapter::marker_block::START,
            slate_cli::adapter::marker_block::END
        ),
    );
    write(
        &env.slate_cache_dir().join("current_theme.lua"),
        "-- old theme\n",
    );
    for path in slate_cli::adapter::GhosttyAdapter
        .integration_candidate_paths_with_env(&env)
        .unwrap()
    {
        write(
            &path,
            format!(
                "font-size = 13\nconfig-file = {}/managed/ghostty/theme.conf\n",
                env.config_dir().display()
            ),
        );
    }
    write(
        &env.xdg_config_home().join("kitty/kitty.conf"),
        format!(
            "font-size 13\ninclude {}/managed/kitty/theme.conf\nlisten_on unix:/tmp/kitty-slate\n",
            env.config_dir().display()
        ),
    );
    write(&env.xdg_config_home().join("alacritty/alacritty.toml"), format!("[general]\nimport = [\"{}/managed/alacritty/theme.toml\", \"user.toml\"]\n[window]\nopacity = 0.8\n", env.config_dir().display()));
    write(
        &env.xdg_config_home().join("starship.toml"),
        "palette = 'slate'\n[palettes.slate]\nred = '#f00'\n[palettes.user]\nred = '#faa'\n",
    );
    write(
        &env.xdg_config_home().join("opencode/tui.json"),
        "{\"theme\":\"system\"}",
    );
    write(
        &env.xdg_config_home().join("opencode/tui.jsonc"),
        "{// comment\n\"theme\":\"system\",\"mouse\":false,}",
    );
    let before_tree = tree_snapshot::tree(home.path());
    let before = profile_files(&env);
    let preview = json_preview(home.path(), true);
    assert_eq!(preview["summary"]["blocked"], 0);
    assert!(preview["summary"]["remove"].as_u64().unwrap() >= 5);
    assert!(preview["summary"]["rewrite"].as_u64().unwrap() >= 8);
    assert_eq!(tree_snapshot::tree(home.path()), before_tree);
    command(home.path())
        .args(["--quiet", "clean"])
        .assert()
        .success();
    let after = profile_files(&env);
    let changes = preview["changes"].as_array().unwrap();
    let mut known = BTreeSet::new();
    for change in changes {
        let path = PathBuf::from(change["path"].as_str().unwrap());
        known.insert(path.clone());
        match change["action"].as_str().unwrap() {
            "remove" => {
                assert!(before.contains_key(&path));
                assert!(!after.contains_key(&path), "{path:?}");
            }
            "rewrite" => {
                assert_ne!(before.get(&path), after.get(&path), "{path:?}");
                assert_eq!(before[&path].0, after[&path].0);
            }
            "unchanged" => assert_eq!(before.get(&path), after.get(&path), "{path:?}"),
            action => panic!("unexpected action {action}"),
        }
    }
    for path in before.keys().chain(after.keys()) {
        if before.get(path) != after.get(path) {
            assert!(known.contains(path), "unlisted change: {path:?}");
        }
    }
    for directory in preview["directories_to_remove"].as_array().unwrap() {
        assert!(!Path::new(directory["path"].as_str().unwrap()).exists());
    }
    assert!(env.config_dir().join("user/linked").is_symlink());
    assert_eq!(tree_snapshot::tree(outside.path()), outside_before);
    let points = list_restore_points_with_env(&env).unwrap();
    assert_eq!(points.len(), 1);
    assert!(execute_restore_with_env(&env, &points[0].id)
        .unwrap()
        .is_fully_successful());
    assert_eq!(profile_files(&env), before);
}

#[test]
fn clean_preview_reports_blockers_without_writes_or_file_content_leaks() {
    for kind in [
        "marker",
        "alacritty",
        "oversize",
        "fifo",
        "link",
        "file-count",
        "directory-depth",
    ] {
        let home = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let env = SlateEnv::with_home(home.path().to_owned());
        write(&outside.path().join("outside"), "PRIVATE_CONTENT_OUTSIDE");
        match kind {
            "marker" => write(
                &env.zshrc_path(),
                format!(
                    "{}\n{}\nPRIVATE_CONTENT_TAIL",
                    slate_cli::adapter::marker_block::END,
                    slate_cli::adapter::marker_block::START
                ),
            ),
            "alacritty" => write(
                &env.xdg_config_home().join("alacritty/alacritty.toml"),
                "PRIVATE_CONTENT_BAD_TOML [",
            ),
            "oversize" => {
                let path = env.managed_file("managed/too-large");
                write(&path, []);
                fs::File::options()
                    .write(true)
                    .open(path)
                    .unwrap()
                    .set_len(8 * 1024 * 1024 + 1)
                    .unwrap();
            }
            "fifo" => {
                let name = std::ffi::CString::new(env.zshrc_path().as_os_str().as_encoded_bytes())
                    .unwrap();
                assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
            }
            "link" => symlink(outside.path().join("outside"), env.zshrc_path()).unwrap(),
            "file-count" => {
                for i in 0..513 {
                    write(&env.managed_file(&format!("managed/file-{i}")), []);
                }
            }
            "directory-depth" => {
                let mut path = env.config_dir().to_owned();
                for _ in 0..66 {
                    path.push("d");
                }
                fs::create_dir_all(path).unwrap();
            }
            _ => unreachable!(),
        }
        let before = tree_snapshot::tree(home.path());
        let outside_before = tree_snapshot::tree(outside.path());
        let report = json_preview(home.path(), false);
        assert!(
            report["scan_complete"] == false || report["summary"]["blocked"].as_u64().unwrap() > 0,
            "{kind}"
        );
        assert_eq!(tree_snapshot::tree(home.path()), before, "{kind}");
        assert_eq!(
            tree_snapshot::tree(outside.path()),
            outside_before,
            "{kind}"
        );
    }
}

#[test]
fn clean_preview_discloses_skipped_parsers_and_escapes_displayed_paths() {
    let home = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(home.path().to_owned());
    write(
        &env.xdg_config_home().join("starship.toml"),
        "PRIVATE_CONTENT [",
    );
    write(
        &env.xdg_config_home().join("opencode/tui.json"),
        "PRIVATE_CONTENT invalid JSON",
    );
    write(
        &env.xdg_config_home().join("alacritty/alacritty.toml"),
        b"PRIVATE_CONTENT\xff",
    );
    write(
        &env.managed_file("managed/中文\u{202e}.conf"),
        "PRIVATE_CONTENT",
    );
    let before = tree_snapshot::tree(home.path());
    let report = json_preview(home.path(), true);
    assert_eq!(report["summary"]["warnings"], 3);
    assert_eq!(report["summary"]["blocked"], 0);
    assert_eq!(report["summary"]["remove"], 1);
    let output = command(home.path())
        .args(["clean", "--dry-run"])
        .assert()
        .success()
        .get_output()
        .clone();
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.contains("中文\\u{202e}.conf"));
    assert!(!text.contains('\u{202e}') && !text.contains('\u{1b}'));
    assert!(!text.contains("PRIVATE_CONTENT"));
    assert_eq!(tree_snapshot::tree(home.path()), before);
}

#[test]
fn clean_preview_uses_custom_roots_and_reports_process_and_storage_boundaries() {
    let home = tempfile::tempdir().unwrap();
    let config_root = home.path().join("custom config");
    let cache_root = home.path().join("custom cache");
    let shell_root = home.path().join("shell");
    let env = SlateEnv::from_vars(|key| match key {
        "HOME" => Some(home.path().as_os_str().to_owned()),
        "XDG_CONFIG_HOME" => Some(config_root.clone().into_os_string()),
        "XDG_CACHE_HOME" => Some(cache_root.clone().into_os_string()),
        "ZDOTDIR" => Some(shell_root.clone().into_os_string()),
        "NVIM_APPNAME" => Some("profiles/work".into()),
        _ => None,
    })
    .unwrap();
    write(
        &env.nvim_config_dir().join("lua/slate/init.lua"),
        "-- private loader",
    );
    write(&env.zshrc_path(), "# keep my shell");
    let custom = || {
        let mut command = assert_cmd::Command::new(assert_cmd::cargo::cargo_bin!("slate"));
        command
            .env_clear()
            .env("HOME", home.path())
            .env("XDG_CONFIG_HOME", &config_root)
            .env("XDG_CACHE_HOME", &cache_root)
            .env("ZDOTDIR", &shell_root)
            .env("NVIM_APPNAME", "profiles/work")
            .env("SSH_CONNECTION", "fixture")
            .env("PATH", home.path().join("bin"))
            .env("NO_COLOR", "1")
            .args(["clean", "--dry-run", "--json"])
            .timeout(Duration::from_secs(5));
        command
    };
    let before = tree_snapshot::tree(home.path());
    let output = custom().assert().success().get_output().clone();
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["watcher_action"], "request_stop");
    assert_eq!(report["terminal_reload"], "skip");
    assert!(report["changes"]
        .as_array()
        .unwrap()
        .iter()
        .any(|entry| entry["path"] == env.zshrc_path().to_str().unwrap()));
    assert!(report["directories_to_remove"]
        .as_array()
        .unwrap()
        .iter()
        .any(|entry| entry["path"] == env.nvim_config_dir().join("lua/slate").to_str().unwrap()));
    for mode in ["nested-cache", "preserved-tier"] {
        let mut command = custom();
        if mode == "nested-cache" {
            command.env("XDG_CACHE_HOME", env.managed_file("managed/cache"));
        } else {
            command.env("ZDOTDIR", env.config_dir().join("user"));
        }
        let output = command.assert().code(1).get_output().clone();
        let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(report["scan_complete"], false, "{mode}");
    }
    assert_eq!(tree_snapshot::tree(home.path()), before);
}
