//! WCAG 2.1 contrast audit test for all registered themes.
//! Fails the build if any theme color falls below the 4.5:1 WCAG AA contrast ratio.
//! Data-driven: iterates `ThemeRegistry::all()` so the audit stays correct as themes
//! are added or renamed.

use slate_cli::theme::ThemeRegistry;
use slate_cli::wcag::audit_palette;

#[test]
fn test_wcag_strict_audit_all_themes_pass() {
    let registry = ThemeRegistry::new().expect("Registry init");
    let ids = registry.list_ids();

    assert!(
        !ids.is_empty(),
        "ThemeRegistry should expose at least one theme"
    );

    let mut total_audited = 0usize;
    let mut total_failures = 0usize;
    let mut failure_details = Vec::new();

    for id in &ids {
        let theme = registry
            .get(id)
            .unwrap_or_else(|| panic!("list_ids() returned {id} but registry.get() missed it"));
        let audits = audit_palette(&theme.palette);

        assert!(
            !audits.is_empty(),
            "{id}: audit_palette should produce at least one result"
        );

        for audit in &audits {
            assert!(
                !audit.color_name.is_empty(),
                "{id}: color_name should not be empty"
            );
            assert!(
                audit.ratio >= 0.0 && audit.ratio <= 22.0,
                "{id}: contrast ratio {} outside sane range [0, 22]",
                audit.ratio
            );
            assert_eq!(
                audit.is_accessible,
                audit.ratio >= 4.5,
                "{id}: is_accessible must match 4.5:1 threshold (ratio {})",
                audit.ratio
            );

            // WCAG STRICT MODE: fail on any inaccessible color
            if !audit.is_accessible {
                total_failures += 1;
                failure_details.push(format!(
                    "  {} color '{}': {:.2}:1 (fg: {}, bg: {})",
                    id, audit.color_name, audit.ratio, audit.foreground, audit.background
                ));
            }
        }

        total_audited += 1;
    }

    eprintln!("\nWCAG audit summary:");
    eprintln!("  Themes audited: {total_audited}");
    eprintln!("  Total failing checks: {total_failures}");
    if total_audited > 0 {
        eprintln!(
            "  Average failures per theme: {:.1}",
            total_failures as f64 / total_audited as f64
        );
    }

    if total_failures > 0 {
        eprintln!("\nFailing colors (strict mode enforcement):");
        for failure in &failure_details {
            eprintln!("{}", failure);
        }
    }

    // WCAG STRICT: Fail the build if any color is inaccessible
    assert_eq!(
        total_failures, 0,
        "WCAG compliance failed: {} color(s) below 4.5:1 threshold. All 20 themes must pass strict audit.",
        total_failures
    );
}

#[test]
fn test_wcag_audit_registry_size() {
    // Sanity check: v2.2 ships 20 themes.
    // This number will change if new themes are added — update the expected
    // value alongside that change.
    let registry = ThemeRegistry::new().expect("Registry init");
    let ids = registry.list_ids();

    assert_eq!(
        ids.len(),
        20,
        "Expected 20 themes registered; got {} ({:?})",
        ids.len(),
        ids
    );
}

#[test]
fn test_wcag_audit_structure_validation() {
    // Use the first registered theme instead of hardcoding a name, to avoid
    // coupling this test to a specific ID spelling.
    let registry = ThemeRegistry::new().expect("Registry init");
    let first_id = registry
        .list_ids()
        .into_iter()
        .next()
        .expect("Registry should have at least one theme");
    let theme = registry.get(&first_id).expect("Registry contains first_id");

    let audit_results = audit_palette(&theme.palette);

    for audit in audit_results {
        assert!(!audit.color_name.is_empty(), "color_name must not be empty");
        assert!(!audit.foreground.is_empty(), "foreground must not be empty");
        assert!(!audit.background.is_empty(), "background must not be empty");
        assert!(
            audit.ratio >= 0.0 && audit.ratio <= 22.0,
            "contrast ratio must be in valid range [0, 22]"
        );
        assert!(
            audit.is_accessible == (audit.ratio >= 4.5),
            "is_accessible must match 4.5:1 threshold"
        );
    }
}
