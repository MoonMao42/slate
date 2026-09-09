use super::*;
use std::{fs, os::unix::fs::MetadataExt};

fn fixture() -> (tempfile::TempDir, SlateEnv) {
    let home = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(home.path().to_owned());
    fs::create_dir_all(env.config_dir()).unwrap();
    fs::write(env.managed_file("current"), "nord\n").unwrap();
    (home, env)
}

#[test]
fn prompt_capture_rejects_directory_retarget_during_source_read() {
    use std::os::unix::fs::symlink;
    for existing in [false, true] {
        let (root, env) = fixture();
        let a = root.path().join("a");
        let b = root.path().join("b");
        fs::create_dir_all(&a).unwrap();
        fs::create_dir_all(&b).unwrap();
        if existing {
            fs::write(a.join("starship.toml"), b"# personal\n").unwrap();
            // Even identical inode/bytes must not hide a destination change.
            fs::hard_link(a.join("starship.toml"), b.join("starship.toml")).unwrap();
        }
        let alias = root.path().join("alias");
        symlink(&a, &alias).unwrap();
        let path = alias.join("starship.toml");
        assert!(File::read(&env, path.clone(), MAX_TOOL_CONFIG_BYTES).is_ok());
        let result = File::read_with(&env, path, MAX_TOOL_CONFIG_BYTES, |path| {
            let original = file_read::read(path, MAX_TOOL_CONFIG_BYTES, Links::Reject).unwrap();
            fs::remove_file(&alias).unwrap();
            symlink(&b, &alias).unwrap();
            Ok(original)
        });
        assert!(
            result.is_err(),
            "retargeted directory was accepted (existing={existing})"
        );
        if existing {
            assert_eq!(fs::read(a.join("starship.toml")).unwrap(), b"# personal\n");
            assert_eq!(fs::read(b.join("starship.toml")).unwrap(), b"# personal\n");
        } else {
            assert!(!a.join("starship.toml").exists());
            assert!(!b.join("starship.toml").exists());
        }
        assert!(!env.slate_cache_dir().exists());
    }
}

#[test]
fn prompt_change_is_scoped_idempotent_and_file_restorable() {
    let (_home, env) = fixture();
    fs::write(
        env.managed_file("config.toml"),
        "# personal\n[preferences]\nsound=false\n",
    )
    .unwrap();
    let original = fs::read(env.managed_file("config.toml")).unwrap();
    let point = PreparedPrompt::capture(&env, PromptStyle::Compact)
        .unwrap()
        .apply()
        .unwrap()
        .unwrap();
    let config = ConfigManager::with_env(&env).unwrap();
    assert_eq!(
        config.get_prompt_style().unwrap(),
        Some(PromptStyle::Compact)
    );
    let paths = [
        env.managed_file("config.toml"),
        env.xdg_config_home().join("starship.toml"),
        env.managed_file("managed/starship/plain.toml"),
    ];
    let inodes: Vec<_> = paths
        .iter()
        .map(|path| fs::metadata(path).unwrap().ino())
        .collect();
    assert!(PreparedPrompt::capture(&env, PromptStyle::Compact)
        .unwrap()
        .apply()
        .unwrap()
        .is_none());
    assert_eq!(
        paths
            .iter()
            .map(|path| fs::metadata(path).unwrap().ino())
            .collect::<Vec<_>>(),
        inodes
    );
    assert_eq!(fs::read(env.managed_file("current")).unwrap(), b"nord\n");
    let point_data = crate::config::get_restore_point_with_env(&env, &point).unwrap();
    assert_eq!(point_data.entries.len(), 3);
    assert!(!point_data.reapplies_theme());
    assert!(crate::config::execute_restore_with_env(&env, &point)
        .unwrap()
        .is_fully_successful());
    assert_eq!(fs::read(env.managed_file("config.toml")).unwrap(), original);
    assert!(!paths[1].exists() && !paths[2].exists());
}

#[test]
fn prompt_review_detects_intervening_edits_before_checkpoint_or_writes() {
    let (_home, env) = fixture();
    let plan = PreparedPrompt::capture(&env, PromptStyle::Minimal).unwrap();
    fs::write(env.managed_file("current"), "catppuccin-latte").unwrap();
    assert!(plan
        .apply()
        .unwrap_err()
        .to_string()
        .contains("changed after review"));
    assert!(!env.slate_cache_dir().exists());
    assert!(!env.xdg_config_home().join("starship.toml").exists());
}

#[test]
fn prompt_style_survives_theme_and_plain_font_regeneration() {
    for style in [
        PromptStyle::Compact,
        PromptStyle::Classic,
        PromptStyle::Focus,
    ] {
        let (_home, env) = fixture();
        PreparedPrompt::capture(&env, style)
            .unwrap()
            .apply()
            .unwrap();
        let config = ConfigManager::with_env(&env).unwrap();
        let seeded: DocumentMut = crate::config::prompt::starter_content(&env)
            .unwrap()
            .parse()
            .unwrap();
        assert!(crate::config::prompt::matches_layout(&seeded.to_string(), style, false).unwrap());
        let registry = ThemeRegistry::new().unwrap();
        config
            .write_shell_integration_file(registry.get("catppuccin-latte").unwrap())
            .unwrap();
        let fallback: DocumentMut =
            fs::read_to_string(env.managed_file("managed/starship/plain.toml"))
                .unwrap()
                .parse()
                .unwrap();
        assert!(crate::config::prompt::matches_layout(&fallback.to_string(), style, true).unwrap());
        assert_eq!(
            fallback["palettes"]["slate"]["blue"].as_str(),
            Some(
                registry
                    .get("catppuccin-latte")
                    .unwrap()
                    .palette
                    .blue
                    .as_str()
            )
        );
        for (_, name, contents) in config.font_shell_files("Menlo").unwrap() {
            if name == "plain.toml" {
                let doc: DocumentMut = contents.parse().unwrap();
                assert!(
                    crate::config::prompt::matches_layout(&doc.to_string(), style, true).unwrap()
                );
            }
        }
    }
}

#[test]
fn focus_change_preserves_personal_modules_and_restores_original_bytes_and_permissions() {
    use std::os::unix::fs::PermissionsExt;
    let (_home, env) = fixture();
    let config = env.xdg_config_home().join("starship.toml");
    let original = "# personal prompt\nformat = '$directory$git_branch$custom$character'\n[git_branch]\nsymbol = 'branch: '\n[custom.keep]\ncommand = 'echo PRIVATE_COMMAND'\nwhen = false\n";
    fs::write(&config, original).unwrap();
    fs::set_permissions(&config, fs::Permissions::from_mode(0o640)).unwrap();
    let preferences = env.managed_file("config.toml");
    fs::write(
        &preferences,
        "# retained settings\n[preferences]\nsound=false\n",
    )
    .unwrap();
    let settings_before = fs::read(&preferences).unwrap();
    let point = PreparedPrompt::capture(&env, PromptStyle::Focus)
        .unwrap()
        .apply()
        .unwrap()
        .unwrap();
    assert_eq!(saved_style(&env).unwrap(), Some(PromptStyle::Focus));
    let updated: DocumentMut = fs::read_to_string(&config).unwrap().parse().unwrap();
    assert_eq!(updated["format"].as_str(), Some("$directory$character"));
    assert_eq!(updated["git_branch"]["symbol"].as_str(), Some("branch: "));
    assert_eq!(
        updated["custom"]["keep"]["command"].as_str(),
        Some("echo PRIVATE_COMMAND")
    );
    assert_eq!(updated["custom"]["keep"]["when"].as_bool(), Some(false));
    assert_eq!(fs::metadata(&config).unwrap().mode() & 0o777, 0o640);
    let inode = fs::metadata(&config).unwrap().ino();
    assert!(PreparedPrompt::capture(&env, PromptStyle::Focus)
        .unwrap()
        .apply()
        .unwrap()
        .is_none());
    assert_eq!(fs::metadata(&config).unwrap().ino(), inode);
    assert!(crate::config::execute_restore_with_env(&env, &point)
        .unwrap()
        .is_fully_successful());
    assert_eq!(fs::read_to_string(&config).unwrap(), original);
    assert_eq!(fs::metadata(&config).unwrap().mode() & 0o777, 0o640);
    assert_eq!(fs::read(preferences).unwrap(), settings_before);
    assert!(!env.managed_file("managed/starship/plain.toml").exists());
    assert_eq!(fs::read(env.managed_file("current")).unwrap(), b"nord\n");
}

#[test]
fn prompt_unknown_saved_preference_is_reported_and_explicit_selection_can_repair_it() {
    let (_home, env) = fixture();
    fs::write(
        env.managed_file("config.toml"),
        "# kept\nprompt = { style = 'PRIVATE_FUTURE', keep = 7 }\n",
    )
    .unwrap();
    assert!(saved_style(&env).is_err());
    let plan = PreparedPrompt::capture(&env, PromptStyle::Minimal).unwrap();
    plan.apply().unwrap();
    assert_eq!(saved_style(&env).unwrap(), Some(PromptStyle::Minimal));
    let doc: DocumentMut = fs::read_to_string(env.managed_file("config.toml"))
        .unwrap()
        .parse()
        .unwrap();
    assert_eq!(doc["prompt"]["keep"].as_integer(), Some(7));
}
