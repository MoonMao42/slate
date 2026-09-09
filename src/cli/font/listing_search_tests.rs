use super::*;
use crate::adapter::font::{FontDiscovery, FontScanIssue};

fn scan() -> FontScanReport {
    FontScanReport {
        fonts: FontDiscovery {
            nerd_fonts: vec![
                "JetBrainsMono Nerd Font".into(),
                "PrivateMono Nerd Font".into(),
            ],
            system_fonts: vec!["Menló".into()],
        },
        ..Default::default()
    }
}

#[test]
fn font_list_search_normalizes_terms_without_changing_choice_evidence() {
    let original = Choices::from_scan(&scan());
    let original = serde_json::to_value(original).unwrap();
    for query in ["mono jetbrains", "JETBRAINS-MONO", "jetbrains_mono mono"] {
        let mut choices = Choices::from_scan(&scan());
        let summary = filter_choices(&mut choices, query);
        assert_eq!(
            (summary.total_candidates, summary.matched_candidates),
            (3, 1)
        );
        assert_eq!(
            (
                summary.total_catalog_entries,
                summary.matched_catalog_entries
            ),
            (4, 1)
        );
        assert_eq!(choices.candidates[0].family, "JetBrainsMono Nerd Font");
        let catalog = serde_json::to_value(&choices.catalog[0]).unwrap();
        assert_eq!(catalog, original["catalog"][0]);
    }
    let mut choices = Choices::from_scan(&scan());
    filter_choices(&mut choices, "mEnLÓ");
    assert_eq!(choices.candidates.len(), 1);
    assert_eq!(choices.candidates[0].family, "Menló");
    assert!(choices.catalog.is_empty());
    for query in ["", " \t "] {
        let mut choices = Choices::from_scan(&scan());
        filter_choices(&mut choices, query);
        assert_eq!(serde_json::to_value(choices).unwrap(), original);
    }
    // Catalog evidence must survive even when a future ID alias does not match
    // the actual candidate family, which the search view correctly hides.
    let mut choices = Choices::from_scan(&scan());
    choices.catalog[0].id = "catalog-only-alias";
    filter_choices(&mut choices, "catalog-only-alias");
    assert!(choices.candidates.is_empty());
    assert_eq!(choices.catalog[0].presence, Presence::CandidateFound);
    assert_eq!(
        choices.catalog[0].matching_candidates,
        ["JetBrainsMono Nerd Font"]
    );
    assert!(!choices.catalog[0].download_offered);
}

#[test]
fn font_list_search_preserves_partial_scan_gates_and_distinguishes_no_matches() {
    let temp = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(temp.path().to_owned());
    let mut scan = scan();
    scan.issues.push(FontScanIssue {
        path: temp.path().join("unreadable"),
        reason: "cannot list font directory",
    });
    scan.omitted_issues = 7;
    let original = serde_json::to_value(report(&env, &scan, None)).unwrap();
    for query in ["fira code", "---", "nothing-matches", "\x1b[2J\u{202e}"] {
        let report = report(&env, &scan, Some(query));
        let text = text_report(&report);
        assert!(!text.contains('\x1b') && !text.contains('\u{202e}'));
        assert!(text.contains("No candidates match this search"));
        assert!(!text.contains("No candidates observed in this scan"));
        let json = serde_json::to_value(report).unwrap();
        assert_eq!(json["search"]["query"], query);
        for field in [
            "scan_complete",
            "search_roots",
            "scan_issues",
            "omitted_issue_count",
        ] {
            assert_eq!(json[field], original[field]);
        }
        for entry in json["catalog"].as_array().unwrap() {
            assert_eq!(entry["presence"], "unknown");
            assert_eq!(entry["download_offered"], false);
        }
    }
    assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 0);
    assert!(validate_list_query(&"x".repeat(256)).is_ok());
    assert!(handle_list_with_query(&env, true, Some(&"x".repeat(257))).is_err());
    assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 0);
}
