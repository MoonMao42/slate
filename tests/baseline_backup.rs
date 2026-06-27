use slate_cli::config::{
    begin_restore_point_baseline, is_baseline_restore_point, list_restore_points_with_env,
    OriginalFileState,
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
    let expected_target_count = if cfg!(target_os = "macos") { 25 } else { 23 };
    assert_eq!(
        baseline.entries.len(),
        expected_target_count,
        "Baseline should capture the full pre-slate target set"
    );
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
    let temp_home = TempDir::new().expect("Failed to create temp home");
    let home = temp_home.path();
    let xdg_ghostty = home.join(".config/ghostty");
    std::fs::create_dir_all(&xdg_ghostty).unwrap();
    std::fs::write(
        xdg_ghostty.join("config.ghostty"),
        "font-family = XDG Current\n",
    )
    .unwrap();
    std::fs::write(xdg_ghostty.join("config"), "font-family = XDG Legacy\n").unwrap();

    #[cfg(target_os = "macos")]
    {
        let app_support = home.join("Library/Application Support/com.mitchellh.ghostty");
        std::fs::create_dir_all(&app_support).unwrap();
        std::fs::write(
            app_support.join("config"),
            "font-family = App Support Legacy\n",
        )
        .unwrap();
    }

    let baseline = begin_restore_point_baseline(home).expect("Failed to create baseline");

    for key in ["ghostty-xdg-config-ghostty", "ghostty-xdg-config"] {
        let entry = baseline
            .entries
            .iter()
            .find(|entry| entry.tool_key == key)
            .unwrap_or_else(|| panic!("missing baseline entry for {key}"));
        assert_eq!(entry.original_state, OriginalFileState::Present);
        assert!(
            entry.backup_path.as_ref().is_some_and(|path| path.exists()),
            "present Ghostty target {key} should have a backup file"
        );
    }

    #[cfg(target_os = "macos")]
    {
        let entry = baseline
            .entries
            .iter()
            .find(|entry| entry.tool_key == "ghostty-macos-app-support-config")
            .expect("missing macOS App Support Ghostty baseline entry");
        assert_eq!(entry.original_state, OriginalFileState::Present);
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
