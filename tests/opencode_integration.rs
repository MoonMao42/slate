//! Adapter writes and failure checks only in disposable OpenCode profiles.
use slate_cli::adapter::{ApplyOutcome, OpencodeAdapter, ToolAdapter};
use slate_cli::env::SlateEnv;
use slate_cli::theme::ThemeRegistry;
use std::fs;
use std::os::unix::fs::{symlink, MetadataExt, PermissionsExt};
use std::path::Path;
use std::time::Duration;

#[path = "support/tree.rs"]
mod tree_snapshot;

fn write(path: &Path, bytes: impl AsRef<[u8]>) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o640)).unwrap();
}

#[test]
fn opencode_application_preserves_jsonc_and_backs_up_exact_bytes_only_on_change() {
    let themes = ThemeRegistry::new().unwrap();
    for already_connected in [false, true] {
        let td = tempfile::tempdir().unwrap();
        let env = SlateEnv::with_home(td.path().to_owned());
        let path = env.xdg_config_home().join("opencode/cli.json");
        let input = if already_connected {
            "// 用户备注\r\n{\"theme\":{\"name\":\"slate-sync\"},\"mouse\":false,}\r\n"
        } else {
            "// 用户备注\r\n{\"$schema\":\"custom\",\"theme\":{\"name\":\"old\"},\"mouse\":false,}\r\n"
        };
        write(&path, input);
        let original_meta = fs::metadata(&path).unwrap();
        let outcome = OpencodeAdapter
            .apply_theme_with_env(themes.get("nord").unwrap(), &env)
            .unwrap();
        assert!(matches!(
            outcome,
            ApplyOutcome::Applied {
                requires_new_shell: false
            }
        ));
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            input.replace("\"old\"", "\"slate-sync\"")
        );
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o640
        );
        if already_connected {
            assert!(!env.slate_cache_dir().join("backups").exists());
            assert_eq!(fs::metadata(&path).unwrap().ino(), original_meta.ino());
            assert_eq!(
                fs::metadata(&path).unwrap().modified().unwrap(),
                original_meta.modified().unwrap()
            );
        } else {
            let backups = tree_snapshot::tree(&env.slate_cache_dir().join("backups"));
            let files: Vec<_> = backups.iter().filter(|(p, _)| p.is_file()).collect();
            assert_eq!(files.len(), 1);
            assert_eq!(files[0].1 .1, input.as_bytes());
            assert_eq!(files[0].1 .0 & 0o777, 0o600);
            assert!(!env.config_dir().exists());
        }
        assert!(OpencodeAdapter::theme_path(&env).is_file());
        let once = tree_snapshot::tree(td.path());
        let meta = fs::metadata(&path).unwrap();
        OpencodeAdapter
            .apply_theme_with_env(themes.get("nord").unwrap(), &env)
            .unwrap();
        assert_eq!(tree_snapshot::tree(td.path()), once);
        assert_eq!(fs::metadata(&path).unwrap().ino(), meta.ino());
        assert_eq!(
            fs::metadata(&path).unwrap().modified().unwrap(),
            meta.modified().unwrap()
        );
    }
}

#[test]
fn opencode_backup_failure_does_not_modify_configuration() {
    let td = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(td.path().to_owned());
    write(
        &env.xdg_config_home().join("opencode/cli.json"),
        "{\"theme\":{\"name\":\"old\"}}",
    );
    write(
        &env.slate_cache_dir().join("backups"),
        "backup directory obstruction",
    );
    let before = tree_snapshot::tree(td.path());
    let themes = ThemeRegistry::new().unwrap();
    assert!(OpencodeAdapter
        .apply_theme_with_env(themes.get("nord").unwrap(), &env)
        .is_err());
    assert_eq!(tree_snapshot::tree(td.path()), before);
}

// The parent imposes a deadline so a FIFO regression cannot stall the runner.
#[test]
fn opencode_input_probe() {
    let Ok(mode) = std::env::var("SLATE_OPENCODE_PROBE") else {
        return;
    };
    let env = SlateEnv::from_process().unwrap();
    let themes = ThemeRegistry::new().unwrap();
    let result = OpencodeAdapter.apply_theme_with_env(themes.get("nord").unwrap(), &env);
    if mode == "isolated" {
        result.unwrap();
    } else {
        assert!(!result.unwrap_err().to_string().contains("PRIVATE_CONTENT"));
    }
}

#[test]
fn opencode_invalid_inputs_are_bounded_read_only_and_do_not_bypass_preferred_paths() {
    for case in [
        "comment",
        "theme",
        "duplicate",
        "deep",
        "utf8",
        "fifo",
        "directory",
        "parent-file",
        "parent-link",
        "link",
        "dangling",
        "large",
        "isolated",
    ] {
        let td = tempfile::tempdir().unwrap();
        let env = SlateEnv::with_home(td.path().to_owned());
        let path = env.xdg_config_home().join("opencode/cli.json");
        // Even with a valid fallback, a blocked preferred entry must fail.
        if !case.starts_with("parent-") {
            write(
                &path.with_extension("jsonc"),
                "{\"theme\":{\"name\":\"slate-sync\"}}",
            );
        }
        let outside = td.path().join("outside.jsonc");
        write(
            &outside,
            "// PRIVATE_CONTENT\n{\"theme\":{\"name\":\"host\"}}",
        );
        match case {
            "comment" => write(&path, "{} /* PRIVATE_CONTENT"),
            "theme" => write(
                &path,
                "{\"theme\":{\"name\":true,\"PRIVATE_CONTENT\":true}}",
            ),
            "duplicate" => write(
                &path,
                "{\"theme\":{\"name\":\"slate-sync\"},\"theme\":{\"name\":\"PRIVATE_CONTENT\"}}",
            ),
            "deep" => write(
                &path,
                format!("{{\"x\":{}0{}}}", "[".repeat(2000), "]".repeat(2000)),
            ),
            "utf8" => write(&path, b"// PRIVATE_CONTENT\xff"),
            "fifo" => {
                let name = std::ffi::CString::new(path.as_os_str().as_encoded_bytes()).unwrap();
                assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
            }
            "directory" => fs::create_dir(&path).unwrap(),
            "parent-file" => write(path.parent().unwrap(), "PRIVATE_CONTENT"),
            "parent-link" => {
                fs::create_dir_all(env.xdg_config_home()).unwrap();
                symlink(td.path().join("missing"), path.parent().unwrap()).unwrap();
            }
            "link" => symlink(&outside, &path).unwrap(),
            "dangling" => symlink(td.path().join("missing"), &path).unwrap(),
            "large" => fs::File::create(&path)
                .unwrap()
                .set_len(8 * 1024 * 1024 + 1)
                .unwrap(),
            "isolated" => write(&path, "{\"theme\":{\"name\":\"slate-sync\"}}"),
            _ => unreachable!(),
        }
        if case == "isolated" {
            OpencodeAdapter
                .apply_theme_with_env(ThemeRegistry::new().unwrap().get("nord").unwrap(), &env)
                .unwrap();
        }
        let before = tree_snapshot::tree(td.path());
        assert_cmd::Command::new(std::env::current_exe().unwrap())
            .env_clear()
            .env("HOME", td.path())
            .env("SLATE_HOME", td.path())
            .env("PATH", td.path().join("bin"))
            .env("SLATE_OPENCODE_PROBE", case)
            .args(["--exact", "opencode_input_probe", "--nocapture"])
            .timeout(Duration::from_secs(5))
            .assert()
            .success();
        assert_eq!(tree_snapshot::tree(td.path()), before, "{case}");
    }
}

#[test]
fn opencode_theme_asset_updates_without_rewriting_config_and_rejects_collisions() {
    let themes = ThemeRegistry::new().unwrap();
    let td = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(td.path().into());
    let config = env.xdg_config_home().join("opencode/cli.json");
    let asset = OpencodeAdapter::theme_path(&env);
    write(&config, "{\"theme\":{\"name\":\"old\"},\"mouse\":false}");
    write(&asset, "{\"personal\":true}");
    let before = tree_snapshot::tree(td.path());
    assert!(OpencodeAdapter
        .apply_theme_with_env(themes.get("nord").unwrap(), &env)
        .is_err());
    assert_eq!(tree_snapshot::tree(td.path()), before);
    fs::remove_file(&asset).unwrap();
    OpencodeAdapter
        .apply_theme_with_env(themes.get("nord").unwrap(), &env)
        .unwrap();
    let old = fs::read(&asset).unwrap();
    let inode = fs::metadata(&config).unwrap().ino();
    OpencodeAdapter
        .apply_theme_with_env(themes.get("catppuccin-latte").unwrap(), &env)
        .unwrap();
    assert_ne!(fs::read(&asset).unwrap(), old);
    assert_eq!(fs::metadata(&config).unwrap().ino(), inode);
    assert_eq!(
        fs::read_to_string(&config).unwrap(),
        "{\"theme\":{\"name\":\"slate-sync\"},\"mouse\":false, \"$schema\": \"https://opencode.ai/v2/cli.json\"}"
    );
    let synced_asset = fs::read(&asset).unwrap();
    let synced_config = fs::read(&config).unwrap();
    assert_cmd::Command::new(assert_cmd::cargo::cargo_bin!("slate"))
        .env("SLATE_HOME", td.path())
        .env("HOME", td.path())
        .env("NO_COLOR", "1")
        .args(["--quiet", "clean"])
        .assert()
        .success();
    assert!(!asset.exists());
    let cleaned: serde_json::Value = serde_json::from_slice(&fs::read(&config).unwrap()).unwrap();
    assert!(cleaned.get("theme").is_none());
    assert_eq!(cleaned["mouse"], false);
    let point = slate_cli::config::list_restore_points_with_env(&env)
        .unwrap()
        .remove(0);
    assert!(slate_cli::config::execute_restore_with_env(&env, &point.id)
        .unwrap()
        .is_fully_successful());
    assert_eq!(fs::read(&asset).unwrap(), synced_asset);
    assert_eq!(fs::read(&config).unwrap(), synced_config);
}
