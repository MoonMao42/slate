use super::*;
use crate::adapter::font::{FontDiscovery, FontScanIssue};
use std::{fs, os::unix::ffi::OsStringExt, path::PathBuf};

fn observed(family: &str) -> FontScanReport {
    FontScanReport {
        fonts: FontDiscovery {
            nerd_fonts: vec![family.into()],
            system_fonts: Vec::new(),
        },
        ..Default::default()
    }
}

#[test]
fn font_preview_catalog_alias_uses_real_resolution_without_installing_or_initializing() {
    let temp = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(temp.path().to_owned());
    let plan = report(&env, "jetbrains-mono", &FontScanReport::default());
    let json = serde_json::to_value(&plan).unwrap();
    assert_eq!(json["resolved_family"], "JetBrains Mono Nerd Font");
    assert_eq!(json["selection_source"], "catalog");
    assert_eq!(json["installation"], "would_request");
    assert_eq!(json["pre_font_checkpoint"], "would_create");
    assert_eq!(json["terminal_reload"], "session_suppressed");
    assert_eq!(json["file_plan_complete"], true);
    assert_eq!(json["execution_readiness_checked"], false);
    assert!(plan
        .files
        .last()
        .unwrap()
        .path
        .path
        .ends_with("/current-font"));
    assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 0);
    assert!(text_report(&plan).contains("would request catalog installation"));
}

#[test]
fn font_preview_partial_evidence_keeps_known_candidate_and_withholds_catalog_plan() {
    let temp = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(temp.path().to_owned());
    let family = "Private Mono Nerd Font";
    let mut scan = observed(family);
    scan.issues.push(FontScanIssue {
        path: PathBuf::from(std::ffi::OsString::from_vec(
            b"/private/\xff\x1b[2J\xe2\x80\xae".to_vec(),
        )),
        reason: "cannot inspect font directory",
    });
    scan.omitted_issues = 5;
    let known = report(&env, family, &scan);
    let json = serde_json::to_value(&known).unwrap();
    assert_eq!(json["file_plan_complete"], true);
    assert_eq!(json["scan_complete"], false);
    assert_eq!(json["installation"], "not_requested");
    assert_eq!(json["scan_issues"][0]["path_is_lossy"], true);
    assert_eq!(json["omitted_issue_count"], 5);
    let unknown = report(&env, "jetbrains-mono", &scan);
    let json = serde_json::to_value(&unknown).unwrap();
    assert_eq!(json["file_plan_complete"], false);
    assert_eq!(json["installation"], "not_planned");
    assert_eq!(json["blocker"]["stage"], "selection");
    assert!(unknown.files.is_empty());
    for plan in [known, unknown] {
        let output = text_report(&plan);
        assert!(!output.contains('\u{1b}') && !output.contains('\u{202e}'));
    }
    assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 0);
}

#[test]
fn font_preview_exact_family_wins_while_ambiguous_alias_stays_blocked() {
    let temp = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(temp.path().to_owned());
    let mut scan = observed("Private-Mono Nerd Font");
    scan.fonts.nerd_fonts.push("Private Mono Nerd Font".into());
    let ambiguous = report(&env, "PrivateMono Nerd Font", &scan);
    assert!(ambiguous
        .blocker
        .unwrap()
        .reason
        .contains("Several font candidates"));
    assert!(ambiguous.files.is_empty());
    for family in &scan.fonts.nerd_fonts {
        let exact = report(&env, family, &scan);
        assert_eq!(exact.resolved_family.as_ref(), Some(family));
        assert!(exact.file_plan_complete);
    }
    assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 0);
}
