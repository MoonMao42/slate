//! WCAG 2.1 contrast audit test for all 18 themes
//! Per and Task 7: Run warning-mode contrast audit on all themes.
//! Non-blocking: warnings logged to stderr, no test failures.

use slate_cli::wcag::{audit_palette, ContrastAudit};
use slate_cli::theme::ThemeRegistry;

#[test]
fn test_wcag_audit_catppuccin_mocha() {
    // Dark theme with full extras
    let registry = ThemeRegistry::new().expect("Registry init");
    let theme = registry.get("catppuccin_mocha").expect("Theme exists");

    let audit_results = audit_palette(&theme.palette);
    
    // Warning-mode: no panic, just verify audit structure
    assert!(!audit_results.is_empty(), "Should produce audit results");
    
    // Count failures (below 4.5:1 threshold)
    let failures = audit_results
        .iter()
        .filter(|a| !a.is_accessible)
        .count();
    
    eprintln!("catppuccin_mocha: {} failures out of {} checks", failures, audit_results.len());
    
    // Verify results structure
    for audit in &audit_results {
        assert!(!audit.color_name.is_empty(), "Color name required");
        assert!(audit.ratio >= 0.0, "Contrast ratio should be non-negative");
    }
}

#[test]
fn test_wcag_audit_catppuccin_latte() {
    // Light theme with full extras
    let registry = ThemeRegistry::new().expect("Registry init");
    let theme = registry.get("catppuccin_latte").expect("Theme exists");

    let audit_results = audit_palette(&theme.palette);
    assert!(!audit_results.is_empty(), "Should produce audit results");
    
    let failures = audit_results.iter().filter(|a| !a.is_accessible).count();
    eprintln!("catppuccin_latte: {} failures out of {} checks", failures, audit_results.len());
}

#[test]
fn test_wcag_audit_tokyo_night() {
    let registry = ThemeRegistry::new().expect("Registry init");
    let theme = registry.get("tokyo_night").expect("Theme exists");

    let audit_results = audit_palette(&theme.palette);
    assert!(!audit_results.is_empty(), "Should produce audit results");
    
    let failures = audit_results.iter().filter(|a| !a.is_accessible).count();
    eprintln!("tokyo_night: {} failures out of {} checks", failures, audit_results.len());
}

#[test]
fn test_wcag_audit_tokyo_night_light() {
    let registry = ThemeRegistry::new().expect("Registry init");
    let theme = registry.get("tokyo_night_light").expect("Theme exists");

    let audit_results = audit_palette(&theme.palette);
    assert!(!audit_results.is_empty(), "Should produce audit results");
    
    let failures = audit_results.iter().filter(|a| !a.is_accessible).count();
    eprintln!("tokyo_night_light: {} failures out of {} checks", failures, audit_results.len());
}

#[test]
fn test_wcag_audit_gruvbox_dark() {
    let registry = ThemeRegistry::new().expect("Registry init");
    let theme = registry.get("gruvbox_dark").expect("Theme exists");

    let audit_results = audit_palette(&theme.palette);
    assert!(!audit_results.is_empty(), "Should produce audit results");
    
    let failures = audit_results.iter().filter(|a| !a.is_accessible).count();
    eprintln!("gruvbox_dark: {} failures out of {} checks", failures, audit_results.len());
}

#[test]
fn test_wcag_audit_gruvbox_light() {
    let registry = ThemeRegistry::new().expect("Registry init");
    let theme = registry.get("gruvbox_light").expect("Theme exists");

    let audit_results = audit_palette(&theme.palette);
    assert!(!audit_results.is_empty(), "Should produce audit results");
    
    let failures = audit_results.iter().filter(|a| !a.is_accessible).count();
    eprintln!("gruvbox_light: {} failures out of {} checks", failures, audit_results.len());
}

#[test]
fn test_wcag_audit_solarized_dark() {
    let registry = ThemeRegistry::new().expect("Registry init");
    let theme = registry.get("solarized_dark").expect("Theme exists");

    let audit_results = audit_palette(&theme.palette);
    assert!(!audit_results.is_empty(), "Should produce audit results");
    
    let failures = audit_results.iter().filter(|a| !a.is_accessible).count();
    eprintln!("solarized_dark: {} failures out of {} checks", failures, audit_results.len());
}

#[test]
fn test_wcag_audit_solarized_light() {
    // Known to have low-contrast foreground (#657b83) vs background (#fdf6e3) = 4.13:1
    // Per plan will be addressed in danger zone compensation work
    let registry = ThemeRegistry::new().expect("Registry init");
    let theme = registry.get("solarized_light").expect("Theme exists");

    let audit_results = audit_palette(&theme.palette);
    assert!(!audit_results.is_empty(), "Should produce audit results");
    
    let failures = audit_results.iter().filter(|a| !a.is_accessible).count();
    eprintln!("solarized_light: {} failures out of {} checks (known issue)", failures, audit_results.len());
    
    // Verify at least one failure is recorded (solarized_light is known to have contrast issues)
    if failures == 0 {
        eprintln!("  WARNING: Expected contrast issues in solarized_light, but none found");
    }
}

#[test]
fn test_wcag_audit_rose_pine() {
    let registry = ThemeRegistry::new().expect("Registry init");
    let theme = registry.get("rose_pine").expect("Theme exists");

    let audit_results = audit_palette(&theme.palette);
    assert!(!audit_results.is_empty(), "Should produce audit results");
    
    let failures = audit_results.iter().filter(|a| !a.is_accessible).count();
    eprintln!("rose_pine: {} failures out of {} checks", failures, audit_results.len());
}

#[test]
fn test_wcag_audit_rose_pine_dawn() {
    let registry = ThemeRegistry::new().expect("Registry init");
    let theme = registry.get("rose_pine_dawn").expect("Theme exists");

    let audit_results = audit_palette(&theme.palette);
    assert!(!audit_results.is_empty(), "Should produce audit results");
    
    let failures = audit_results.iter().filter(|a| !a.is_accessible).count();
    eprintln!("rose_pine_dawn: {} failures out of {} checks", failures, audit_results.len());
}

#[test]
fn test_wcag_audit_base16_ocean() {
    let registry = ThemeRegistry::new().expect("Registry init");
    let theme = registry.get("base16_ocean").expect("Theme exists");

    let audit_results = audit_palette(&theme.palette);
    assert!(!audit_results.is_empty(), "Should produce audit results");
    
    let failures = audit_results.iter().filter(|a| !a.is_accessible).count();
    eprintln!("base16_ocean: {} failures out of {} checks", failures, audit_results.len());
}

#[test]
fn test_wcag_audit_nord() {
    let registry = ThemeRegistry::new().expect("Registry init");
    let theme = registry.get("nord").expect("Theme exists");

    let audit_results = audit_palette(&theme.palette);
    assert!(!audit_results.is_empty(), "Should produce audit results");
    
    let failures = audit_results.iter().filter(|a| !a.is_accessible).count();
    eprintln!("nord: {} failures out of {} checks", failures, audit_results.len());
}

#[test]
fn test_wcag_audit_dracula() {
    let registry = ThemeRegistry::new().expect("Registry init");
    let theme = registry.get("dracula").expect("Theme exists");

    let audit_results = audit_palette(&theme.palette);
    assert!(!audit_results.is_empty(), "Should produce audit results");
    
    let failures = audit_results.iter().filter(|a| !a.is_accessible).count();
    eprintln!("dracula: {} failures out of {} checks", failures, audit_results.len());
}

#[test]
fn test_wcag_audit_monokai() {
    let registry = ThemeRegistry::new().expect("Registry init");
    let theme = registry.get("monokai").expect("Theme exists");

    let audit_results = audit_palette(&theme.palette);
    assert!(!audit_results.is_empty(), "Should produce audit results");
    
    let failures = audit_results.iter().filter(|a| !a.is_accessible).count();
    eprintln!("monokai: {} failures out of {} checks", failures, audit_results.len());
}

#[test]
fn test_wcag_audit_one_dark() {
    let registry = ThemeRegistry::new().expect("Registry init");
    let theme = registry.get("one_dark").expect("Theme exists");

    let audit_results = audit_palette(&theme.palette);
    assert!(!audit_results.is_empty(), "Should produce audit results");
    
    let failures = audit_results.iter().filter(|a| !a.is_accessible).count();
    eprintln!("one_dark: {} failures out of {} checks", failures, audit_results.len());
}

#[test]
fn test_wcag_audit_kanagawa() {
    let registry = ThemeRegistry::new().expect("Registry init");
    let theme = registry.get("kanagawa").expect("Theme exists");

    let audit_results = audit_palette(&theme.palette);
    assert!(!audit_results.is_empty(), "Should produce audit results");
    
    let failures = audit_results.iter().filter(|a| !a.is_accessible).count();
    eprintln!("kanagawa: {} failures out of {} checks", failures, audit_results.len());
}

#[test]
fn test_wcag_audit_everforest() {
    let registry = ThemeRegistry::new().expect("Registry init");
    let theme = registry.get("everforest").expect("Theme exists");

    let audit_results = audit_palette(&theme.palette);
    assert!(!audit_results.is_empty(), "Should produce audit results");
    
    let failures = audit_results.iter().filter(|a| !a.is_accessible).count();
    eprintln!("everforest: {} failures out of {} checks", failures, audit_results.len());
}

#[test]
fn test_wcag_audit_github_dark() {
    let registry = ThemeRegistry::new().expect("Registry init");
    let theme = registry.get("github_dark").expect("Theme exists");

    let audit_results = audit_palette(&theme.palette);
    assert!(!audit_results.is_empty(), "Should produce audit results");
    
    let failures = audit_results.iter().filter(|a| !a.is_accessible).count();
    eprintln!("github_dark: {} failures out of {} checks", failures, audit_results.len());
}

#[test]
fn test_wcag_audit_all_18_themes_complete() {
    // Summary test: verify all 18 themes can be audited
    let registry = ThemeRegistry::new().expect("Registry init");
    
    let theme_ids = vec![
        "catppuccin_mocha",
        "catppuccin_latte",
        "tokyo_night",
        "tokyo_night_light",
        "gruvbox_dark",
        "gruvbox_light",
        "solarized_dark",
        "solarized_light",
        "rose_pine",
        "rose_pine_dawn",
        "base16_ocean",
        "nord",
        "dracula",
        "monokai",
        "one_dark",
        "kanagawa",
        "everforest",
        "github_dark",
    ];
    
    let mut total_audited = 0;
    let mut total_failures = 0;
    
    for theme_id in theme_ids {
        let theme = registry.get(theme_id).expect(&format!("Theme {} exists", theme_id));
        let audit_results = audit_palette(&theme.palette);
        
        let failures = audit_results.iter().filter(|a| !a.is_accessible).count();
        total_audited += 1;
        total_failures += failures;
        
        eprintln!("{}: {} failures", theme_id, failures);
    }
    
    eprintln!("\nWCAG Audit Summary:");
    eprintln!("  Total themes audited: {}", total_audited);
    eprintln!("  Total failing checks: {}", total_failures);
    eprintln!("  Average failures per theme: {:.1}", total_failures as f64 / total_audited as f64);
    
    // Verify we audited all 18 themes
    assert_eq!(total_audited, 18, "Should audit exactly 18 themes");
}

#[test]
fn test_wcag_audit_structure_validation() {
    // Verify ContrastAudit struct fields are properly populated
    let registry = ThemeRegistry::new().expect("Registry init");
    let theme = registry.get("catppuccin_mocha").expect("Theme exists");

    let audit_results = audit_palette(&theme.palette);
    
    for audit in audit_results {
        // Verify all fields are populated
        assert!(!audit.color_name.is_empty(), "color_name must not be empty");
        assert!(!audit.foreground.is_empty(), "foreground must not be empty");
        assert!(!audit.background.is_empty(), "background must not be empty");
        assert!(audit.ratio >= 0.0 && audit.ratio <= 22.0, "contrast ratio must be in valid range [0, 22]");
        assert!(audit.is_accessible == (audit.ratio >= 4.5), "is_accessible must match 4.5:1 threshold");
    }
}
