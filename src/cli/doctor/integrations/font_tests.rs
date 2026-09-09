use super::*;
use crate::adapter::font::{FontDiscovery, FontScanIssue};
use std::os::unix::ffi::OsStringExt;

fn report() -> Report {
    Report {
        schema_version: 1,
        target: "font".into(),
        scope: "test",
        checks: Vec::new(),
        version_probe: None,
        font_inventory: None,
        font_references: None,
    }
}

#[test]
fn font_doctor_references_serialize_partial_positive_evidence_and_lossy_paths() {
    let path =
        std::path::PathBuf::from(std::ffi::OsString::from_vec(b"/entry/\xff\x1b[2J".to_vec()));
    let observed = references::References {
        terminal: Terminal::Ghostty,
        managed: path.clone(),
        entries: vec![
            references::Entry {
                path: path.clone(),
                state: State::Found,
                reason: None,
            },
            references::Entry {
                path: "/entry/bad".into(),
                state: State::Uninspectable,
                reason: Some("cannot read".into()),
            },
        ],
        path_error: None,
    };
    let json = serde_json::to_value(ReferenceSummary::from(observed)).unwrap();
    assert_eq!(json["terminal"], "ghostty");
    assert_eq!(json["state"], "found");
    assert_eq!(json["inspection_complete"], false);
    assert_eq!(json["managed_path_is_lossy"], true);
    assert_eq!(json["entries"][0]["path_is_lossy"], true);
    assert_eq!(json["entries"][1]["state"], "uninspectable");
}

#[test]
fn font_doctor_inventory_keeps_positive_partial_and_unknown_absence_distinct() {
    let env = SlateEnv::with_home("/private-fixture".into());
    for (matched, partial, system, expected) in [
        (true, false, false, "family_candidate_found"),
        (true, true, false, "family_candidate_found"),
        (true, false, true, "family_candidate_found"),
        (false, true, false, "family_availability_unknown"),
        (false, false, false, "family_candidate_not_found"),
    ] {
        let mut scan = FontScanReport::default();
        if matched {
            let families = if system {
                &mut scan.fonts.system_fonts
            } else {
                &mut scan.fonts.nerd_fonts
            };
            families.push("Private-Font Nerd Font".into());
        }
        if partial {
            scan.omitted_issues = 4;
        }
        let mut report = report();
        inventory(&mut report, &env, Some("PrivateFont Nerd Font"), &scan);
        assert!(report
            .checks
            .iter()
            .any(|check| check.code == Some(expected)));
        assert_eq!(report.font_inventory.unwrap().scan_complete, !partial);
    }
}

#[test]
fn font_doctor_inventory_reports_omissions_and_lossy_paths_without_terminal_injection() {
    let env = SlateEnv::with_home("/private-fixture".into());
    let scan = FontScanReport {
        fonts: FontDiscovery::default(),
        issues: vec![FontScanIssue {
            path: std::ffi::OsString::from_vec(b"/font/\xff\x1b[2J".to_vec()).into(),
            reason: "cannot inspect font directory",
        }],
        omitted_issues: 17,
    };
    let mut report = report();
    inventory(&mut report, &env, None, &scan);
    let issue = report
        .checks
        .iter()
        .find(|check| check.code == Some("scan_issue"))
        .unwrap();
    assert!(issue.path_is_lossy);
    assert!(report
        .checks
        .iter()
        .any(|check| check.code == Some("scan_issues_omitted")));
    let text = super::super::text_report(&report);
    assert!(!text.contains('\u{1b}'));
    let json = serde_json::to_value(&report).unwrap();
    assert_eq!(json["font_inventory"]["reported_issue_count"], 1);
    assert_eq!(json["font_inventory"]["omitted_issue_count"], 17);
    assert!(json["font_inventory"]["selected_family"].is_null());
}
