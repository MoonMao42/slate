//! OpenCode 2's global CLI path is shared by apply, doctor, clean and restore.
use slate_cli::{
    cli::theme_apply::ThemeApplyCoordinator,
    config::{execute_restore_with_env, get_restore_point_with_env},
    env::SlateEnv,
    theme::ThemeRegistry,
};
use std::{fs, os::unix::fs::PermissionsExt, path::Path, time::Duration};

fn write(path: &Path, content: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, content).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o640)).unwrap();
}

#[test]
fn opencode_cli_path_and_theme_mode_survive_apply_clean_and_restore() {
    for custom_xdg in [false, true] {
        let home = tempfile::tempdir().unwrap();
        let root = home.path().join(if custom_xdg {
            "中文配置"
        } else {
            ".config"
        });
        let env = SlateEnv::from_vars(|key| match key {
            "HOME" => Some(home.path().as_os_str().to_owned()),
            "XDG_CONFIG_HOME" => Some(root.as_os_str().to_owned()),
            _ => None,
        })
        .unwrap();
        let path = root.join("opencode/cli.json");
        let original =
            "// user note\n{\"theme\":{\"name\":\"custom\",\"mode\":\"light\"},\"mouse\":false}\n";
        write(&path, original);
        let themes = ThemeRegistry::new().unwrap();
        let report = ThemeApplyCoordinator::new(&env)
            .apply_to_tools(themes.get("nord").unwrap(), &["opencode".into()])
            .unwrap();
        report.ensure_no_failures().unwrap();
        assert_eq!(report.applied_count(), 1);
        let restore_id = report.restore_point_id.unwrap();
        let point = get_restore_point_with_env(&env, &restore_id).unwrap();
        assert!(point
            .entries
            .iter()
            .any(|entry| entry.original_path == path));
        let synced = fs::read_to_string(&path).unwrap();
        assert!(synced.contains("\"name\":\"slate-sync\",\"mode\":\"light\""));
        assert!(synced.contains("// user note"));
        let command = || {
            let mut cmd = assert_cmd::Command::new(assert_cmd::cargo::cargo_bin!("slate"));
            cmd.env_clear()
                .env("HOME", home.path())
                .env("XDG_CONFIG_HOME", &root)
                .env("PATH", home.path().join("bin"))
                .env("NO_COLOR", "1")
                .current_dir(home.path())
                .timeout(Duration::from_secs(5));
            cmd
        };
        let output = command()
            .args(["doctor", "opencode", "--json"])
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();
        let doctor: serde_json::Value = serde_json::from_slice(&output).unwrap();
        assert!(doctor["checks"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["code"] == "selected_config" && c["path"].as_str() == path.to_str()));
        assert_eq!(fs::read_to_string(&path).unwrap(), synced);
        command().args(["--quiet", "clean"]).assert().success();
        let cleaned = fs::read_to_string(&path).unwrap();
        assert!(cleaned.contains("\"mode\":\"light\"") && cleaned.contains("// user note"));
        assert!(!cleaned.contains("slate-sync"));
        assert!(execute_restore_with_env(&env, &restore_id)
            .unwrap()
            .is_fully_successful());
        assert_eq!(fs::read_to_string(&path).unwrap(), original);
        assert_eq!(
            fs::metadata(path).unwrap().permissions().mode() & 0o777,
            0o640
        );
    }
}
