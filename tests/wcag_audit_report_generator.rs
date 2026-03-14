//! Generate a detailed WCAG audit report for all registered themes.
//! Run with: cargo test --test wcag_audit_report_generator -- --nocapture

use slate_cli::theme::ThemeRegistry;
use slate_cli::wcag::audit_palette;
use std::fs;
use std::path::PathBuf;

#[test]
fn generate_wcag_strict_audit_report() {
    let registry = ThemeRegistry::new().expect("Registry init");
    let mut all_failures = Vec::new();

    // Collect all failures
    for theme in registry.all() {
        let audits = audit_palette(&theme.palette);

        for audit in audits {
            if !audit.is_accessible {
                all_failures.push((
                    theme.id.clone(),
                    audit.color_name,
                    audit.foreground,
                    audit.background,
                    audit.ratio,
                ));
            }
        }
    }

    // Sort for consistency
    all_failures.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));

    // Generate markdown report
    let mut report = String::new();
    report.push_str("# WCAG Strict Audit Report\n\n");
    report.push_str("**Generated from:** Current theme sources via ThemeRegistry\n\n");
    report.push_str("This report captures all ANSI 0-15 slots and semantic UI colors that fail\n");
    report.push_str(
        "the WCAG 4.5:1 contrast ratio threshold against their respective theme backgrounds.\n\n",
    );

    // Summary stats
    let theme_count = registry.list_ids().len();
    let failure_count = all_failures.len();
    report.push_str(&format!("**Total Themes Audited:** {}\n", theme_count));
    report.push_str(&format!("**Total Failures Found:** {}\n\n", failure_count));

    // Group by theme
    let mut current_theme = String::new();
    for (theme_id, color_name, fg, bg, ratio) in &all_failures {
        if theme_id != &current_theme {
            if !current_theme.is_empty() {
                report.push('\n');
            }
            report.push_str(&format!("## {}\n\n", theme_id));
            current_theme = theme_id.clone();
        }

        let severity = if ratio < &3.0 { "CRITICAL" } else { "FAIL" };
        report.push_str(&format!(
            "- **{}**: {} (ratio: {:.2}:1) — fg: {}, bg: {}\n",
            color_name, severity, ratio, fg, bg
        ));
    }

    // Write the audit report to Cargo's build artifact directory.
    let reports_dir = PathBuf::from("target/reports");
    fs::create_dir_all(&reports_dir).expect("Failed to create reports directory");
    let report_path = reports_dir.join("wcag-audit.md");
    fs::write(&report_path, report).expect("Failed to write audit report");

    println!("\nAudit report written to: {:?}", report_path);
    println!(
        "Total themes: {}, Total failures: {}",
        theme_count, failure_count
    );
}
