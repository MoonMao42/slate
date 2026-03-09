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
    assert_eq!(
        baseline.entries.len(),
        19,
        "Baseline should capture the full pre-slate target set"
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
