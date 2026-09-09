use slate_cli::config::{
    begin_restore_point_baseline, execute_restore_with_env, is_baseline_restore_point,
    list_restore_points_with_env, OriginalFileState,
};
use slate_cli::env::SlateEnv;
use tempfile::TempDir;

/// Test that baseline creation marks restore point correctly
#[test]
fn test_baseline_created_before_first_setup() {
    let temp_home = TempDir::new().expect("Failed to create temp home");
    let home = temp_home.path();

    // Create a baseline snapshot
    let baseline = begin_restore_point_baseline(home).expect("Failed to create baseline");

    // Verify baseline was created and marked with is_baseline=true
    assert!(
        is_baseline_restore_point(&baseline),
        "Restore point should be marked as baseline"
    );
}

/// Test that baseline has correct metadata
#[test]
fn test_baseline_has_correct_metadata() {
    let temp_home = TempDir::new().expect("Failed to create temp home");
    let home = temp_home.path();

    let baseline = begin_restore_point_baseline(home).expect("Failed to create baseline");

    // Baseline should snapshot the fixed target list, including absent files.
    let expected_target_count = if cfg!(target_os = "macos") { 43 } else { 41 };
    assert_eq!(
        baseline.entries.len(),
        expected_target_count,
        "Baseline should capture the full pre-slate target set"
    );
    for path in [
        home.join(".bash_login"),
        home.join(".profile"),
        home.join(".config/alacritty/alacritty.toml"),
        home.join(".config/alacritty.toml"),
        home.join(".alacritty.toml"),
        home.join(".config/slate/managed/ghostty/opacity.conf"),
        home.join(".config/slate/managed/ghostty/blur.conf"),
        home.join(".config/slate/managed/alacritty/opacity.toml"),
        home.join(".config/slate/managed/kitty/opacity.conf"),
        home.join(".config/zellij/config.kdl"),
        home.join(".config/zellij/themes/slate-sync.kdl"),
        home.join(".config/btop/btop.conf"),
        home.join(".config/btop/themes/slate-sync.theme"),
        home.join(".config/yazi/theme.toml"),
        home.join(".config/yazi/flavors/slate-sync.yazi/flavor.toml"),
        home.join(".config/yazi/flavors/slate-sync.yazi/tmtheme.xml"),
    ] {
        assert!(baseline
            .entries
            .iter()
            .any(|entry| entry.original_path == path
                && entry.original_state == OriginalFileState::Absent));
    }
    assert!(
        baseline
            .entries
            .iter()
            .any(|entry| entry.tool_key == "ghostty-xdg-config-ghostty"),
        "Baseline should track Ghostty's current XDG config.ghostty path"
    );
    assert!(
        baseline
            .entries
            .iter()
            .any(|entry| entry.tool_key == "ghostty-xdg-config"),
        "Baseline should track Ghostty's legacy XDG config path"
    );
    #[cfg(target_os = "macos")]
    assert!(
        baseline
            .entries
            .iter()
            .any(|entry| entry.tool_key == "ghostty-macos-app-support-config"),
        "Baseline should track Ghostty's macOS App Support config path"
    );
    assert!(
        baseline
            .entries
            .iter()
            .any(|entry| entry.tool_key == "kitty"),
        "Baseline should track kitty.conf so restore can roll back slate-added include/remote-control lines"
    );
    assert!(
        baseline
            .entries
            .iter()
            .any(|entry| entry.tool_key == "nvim-init-lua"),
        "Baseline should track nvim init.lua so restore can remove the slate marker block"
    );
    assert!(
        baseline
            .entries
            .iter()
            .any(|entry| entry.tool_key == "nvim-init-vim"),
        "Baseline should track nvim init.vim so restore can remove the slate marker block"
    );
    assert!(
        baseline
            .entries
            .iter()
            .any(|entry| entry.tool_key == "bashrc"),
        "Baseline should track .bashrc"
    );
    assert!(
        baseline
            .entries
            .iter()
            .any(|entry| entry.tool_key == "bash-profile"),
        "Baseline should track .bash_profile (macOS login bash)"
    );
    assert!(
        baseline
            .entries
            .iter()
            .any(|entry| entry.tool_key == "fish-loader"),
        "Baseline should track the fish conf.d loader"
    );
    assert!(
        baseline
            .entries
            .iter()
            .any(|entry| entry.tool_key == "slate-auto-watcher"),
        "Baseline should track the managed auto-theme watcher"
    );
    assert!(
        baseline.entries.iter().any(|entry| {
            entry.tool_key == "slate-appearance-helper"
                && entry.original_path
                    == home.join(".config/slate/managed/bin/slate-appearance-helper")
                && entry.original_state == OriginalFileState::Absent
        }),
        "Baseline should record the event-only appearance helper as absent before setup"
    );
    assert!(
        baseline
            .entries
            .iter()
            .any(|entry| entry.tool_key == "slate-shell-bash"),
        "Baseline should track managed bash shell env"
    );
    assert!(
        baseline
            .entries
            .iter()
            .any(|entry| entry.tool_key == "slate-shell-fish"),
        "Baseline should track managed fish shell env"
    );
    assert!(
        baseline
            .entries
            .iter()
            .any(|entry| matches!(entry.original_state, OriginalFileState::Absent)),
        "Fresh baseline should record missing files as absent entries"
    );

    // Baseline theme name should indicate pre-slate
    assert!(
        baseline.theme_name.contains("baseline") || baseline.theme_name.contains("pre"),
        "Baseline theme name should indicate pre-slate state, got: {}",
        baseline.theme_name
    );

    // Baseline ID should be non-empty
    assert!(!baseline.id.is_empty(), "Baseline ID should not be empty");
}

#[test]
fn test_baseline_snapshots_all_existing_ghostty_candidates() {
    use slate_cli::adapter::{GhosttyAdapter, ToolAdapter};
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    let temp_home = TempDir::new().expect("Failed to create temp home");
    let home = temp_home.path();
    let env = SlateEnv::with_home(home.to_path_buf());
    let xdg_ghostty = home.join(".config/ghostty");
    let mut fixtures = vec![
        ("ghostty-xdg-config", xdg_ghostty.join("config")),
        (
            "ghostty-xdg-config-ghostty",
            xdg_ghostty.join("config.ghostty"),
        ),
    ];
    if cfg!(target_os = "macos") {
        let directory = home.join("Library/Application Support/com.mitchellh.ghostty");
        fixtures.extend([
            ("ghostty-macos-app-support-config", directory.join("config")),
            (
                "ghostty-macos-app-support-config-ghostty",
                directory.join("config.ghostty"),
            ),
        ]);
    }
    let managed = env.managed_file("managed/ghostty/theme.conf");
    let fixtures: Vec<_> = fixtures
        .into_iter()
        .enumerate()
        .map(|(index, (key, path))| {
            let manual = format!("# private {key}\nfont-family = Font {index}\n");
            let original = format!("{manual}config-file = \"{}\"\n", managed.display());
            let mode = if index % 2 == 0 { 0o600 } else { 0o640 };
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, &original).unwrap();
            fs::set_permissions(&path, fs::Permissions::from_mode(mode)).unwrap();
            (key, path, manual, original, mode)
        })
        .collect();

    let baseline = begin_restore_point_baseline(home).expect("Failed to create baseline");

    for (key, path, _, original, mode) in &fixtures {
        let entry = baseline
            .entries
            .iter()
            .find(|entry| entry.tool_key == *key)
            .unwrap_or_else(|| panic!("missing baseline entry for {key}"));
        assert_eq!(entry.original_state, OriginalFileState::Present);
        assert_eq!(&entry.original_path, path, "{key}");
        assert_eq!(entry.unix_mode, Some(*mode), "{key}");
        assert_eq!(
            fs::read(entry.backup_path.as_ref().unwrap()).unwrap(),
            original.as_bytes(),
            "{key}"
        );
    }

    let selected = &fixtures.last().unwrap().1;
    assert_eq!(
        &GhosttyAdapter
            .integration_config_path_with_env(&env)
            .unwrap(),
        selected
    );
    let theme = slate_cli::theme::catppuccin::catppuccin_mocha().unwrap();
    GhosttyAdapter.apply_theme_with_env(&theme, &env).unwrap();
    GhosttyAdapter::apply_font_only(&env, "Private Test Font").unwrap();
    let mut applied = Vec::new();
    for (_, path, manual, _, mode) in &fixtures {
        let content = fs::read_to_string(path).unwrap();
        assert!(content.starts_with(manual));
        if path == selected {
            assert_eq!(content.matches("config-file = ").count(), 4);
            for name in ["theme.conf", "opacity.conf", "blur.conf", "font.conf"] {
                assert!(content.contains(name), "{name}");
            }
        } else {
            assert_eq!(&content, manual);
        }
        assert_eq!(
            fs::metadata(path).unwrap().permissions().mode() & 0o777,
            *mode
        );
        applied.push(content);
    }

    // Restore and undo use the recorded paths/keys, never a candidate's index.
    let restored = execute_restore_with_env(&env, &baseline.id).unwrap();
    assert!(restored.is_fully_successful(), "{restored:?}");
    for (key, path, _, original, mode) in &fixtures {
        assert_eq!(fs::read(path).unwrap(), original.as_bytes(), "{key}");
        assert_eq!(
            fs::metadata(path).unwrap().permissions().mode() & 0o777,
            *mode
        );
    }
    let undone = execute_restore_with_env(&env, &restored.pre_restore_point_id).unwrap();
    assert!(undone.is_fully_successful(), "{undone:?}");
    for ((key, path, _, _, mode), content) in fixtures.iter().zip(applied) {
        assert_eq!(fs::read(path).unwrap(), content.as_bytes(), "{key}");
        assert_eq!(
            fs::metadata(path).unwrap().permissions().mode() & 0o777,
            *mode
        );
    }
}

/// Test that baseline protection flag is set
#[test]
fn test_baseline_protection_flag() {
    let temp_home = TempDir::new().expect("Failed to create temp home");
    let home = temp_home.path();

    let baseline = begin_restore_point_baseline(home).expect("Failed to create baseline");

    // This baseline should be protected by reset logic
    assert!(
        baseline.is_baseline,
        "is_baseline flag should be true for baseline restore point"
    );
}

/// Test that baseline can be created multiple times (generates unique IDs)
#[test]
fn test_baseline_multiple_creation() {
    let temp_home = TempDir::new().expect("Failed to create temp home");
    let home = temp_home.path();

    // Create baseline first time
    let baseline_1 = begin_restore_point_baseline(home).expect("Failed to create baseline (1st)");

    // Try to create baseline again
    let baseline_2 = begin_restore_point_baseline(home).expect("Failed to create baseline (2nd)");

    // Both should be valid baselines
    assert!(is_baseline_restore_point(&baseline_1));
    assert!(is_baseline_restore_point(&baseline_2));

    // Both have valid unique IDs
    assert!(!baseline_1.id.is_empty());
    assert!(!baseline_2.id.is_empty());
    assert_ne!(baseline_1.id, baseline_2.id);
}

#[test]
fn test_baseline_is_listed_via_injected_env() {
    let temp_home = TempDir::new().expect("Failed to create temp home");
    let env = SlateEnv::with_home(temp_home.path().to_path_buf());

    let baseline =
        begin_restore_point_baseline(temp_home.path()).expect("Failed to create baseline");
    let restore_points = list_restore_points_with_env(&env).expect("Failed to list restore points");

    assert!(restore_points
        .iter()
        .any(|point| point.id == baseline.id && point.is_baseline));
}
