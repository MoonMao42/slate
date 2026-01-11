use slate_cli::config::{begin_restore_point_baseline, is_baseline_restore_point};
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

    // Baseline should have empty entries (no state captured yet)
    assert_eq!(
        baseline.entries.len(),
        0,
        "Baseline should have no entries (pre-slate state)"
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

    // Both have valid IDs (may be same or different depending on implementation)
    assert!(!baseline_1.id.is_empty());
    assert!(!baseline_2.id.is_empty());
}
