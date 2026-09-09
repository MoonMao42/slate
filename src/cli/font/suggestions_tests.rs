use super::*;
use crate::adapter::font::{FontScanIssue, FontScanReport};
use crate::cli::font::{
    resolve_font_choice_with_discovery, resolve_font_choice_with_scan, ResolvedFontChoice,
};

fn discovery(names: &[&str]) -> FontDiscovery {
    FontDiscovery {
        nerd_fonts: names.iter().map(|name| (*name).into()).collect(),
        system_fonts: Vec::new(),
    }
}

#[test]
fn font_suggestions_cover_typos_catalog_ids_and_observed_exact_names_without_selecting() {
    for query in ["hakc", "fira-cdoe", "jetbra", "iosevka-trm"] {
        let fonts = discovery(&[]);
        let (items, limited) = nearby(query, &fonts);
        assert_eq!(items.len(), 1, "{query}");
        assert_eq!(items[0].source, Source::Catalog);
        assert!(!limited);
        let error = resolve_font_choice_with_discovery(query, &fonts)
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("Nearby exact family names") && error.contains("may download"),
            "{error}"
        );
    }
    let fonts = discovery(&["Hack Nerd Font", "Hack Nerd Font", "Invalid\nNerd Font"]);
    let (items, _) = nearby("hakc", &fonts);
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].family, "Hack Nerd Font");
    assert_eq!(items[0].source, Source::Observed);
    assert!(resolve_font_choice_with_discovery("hakc", &fonts).is_err());
    assert_eq!(
        resolve_font_choice_with_discovery("Hack Nerd Font", &fonts).unwrap(),
        ResolvedFontChoice::Installed("Hack Nerd Font".into())
    );
    for query in ["ha", "completely-unrelated", "🪐", &"x".repeat(65)] {
        assert!(nearby(query, &fonts).0.is_empty());
    }
}

#[test]
fn font_suggestions_list_ambiguous_names_without_changing_exact_or_partial_scan_rules() {
    let mut fonts = discovery(&[
        "Twin-Mono Nerd Font",
        "Twin Mono Nerd Font",
        "Twin_Mono Nerd Font",
        "Twin.Mono Nerd Font",
        "Twin\"Mono Nerd Font",
    ]);
    let error = resolve_font_choice_with_discovery("Twin:Mono Nerd Font", &fonts)
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("Matching exact family names") && error.contains("and 2 more"),
        "{error}"
    );
    assert_eq!(
        error
            .lines()
            .filter(|line| line.starts_with("  - "))
            .count(),
        3
    );
    fonts.nerd_fonts.reverse();
    assert_eq!(
        resolve_font_choice_with_discovery("Twin:Mono Nerd Font", &fonts)
            .unwrap_err()
            .to_string(),
        error
    );
    let scan = FontScanReport {
        fonts,
        issues: vec![FontScanIssue {
            path: "/private/unreadable-font-root".into(),
            reason: "cannot list font directory",
        }],
        omitted_issues: 0,
    };
    for family in &scan.fonts.nerd_fonts {
        assert_eq!(
            resolve_font_choice_with_scan(family, &scan).unwrap(),
            ResolvedFontChoice::Installed(family.clone())
        );
    }
    for query in ["hakc", "hack", "Twin:Mono Nerd Font"] {
        let message = resolve_font_choice_with_scan(query, &scan)
            .unwrap_err()
            .to_string();
        assert!(message.contains("discovery is incomplete"), "{message}");
        assert!(!message.contains("may download") && !message.contains("not found"));
    }
}

#[test]
fn font_suggestions_bound_work_output_and_escape_display_without_changing_family_data() {
    let name = "Private\u{202e}Mono Nerd Font";
    let fonts = discovery(&[name]);
    let message = not_found("Privat\u{202e}Mono Nerd Font", &fonts).to_string();
    assert!(!message.contains('\u{202e}'));
    assert!(message.contains("\\u{202e}"));
    assert_eq!(fonts.nerd_fonts[0], name);
    assert_eq!(
        resolve_font_choice_with_discovery(name, &fonts).unwrap(),
        ResolvedFontChoice::Installed(name.into())
    );
    let mut fonts = discovery(&[]);
    fonts.nerd_fonts = (0..MAX_OBSERVED + 10)
        .map(|index| format!("OtherCandidate{index:04} Nerd Font"))
        .collect();
    let message = not_found("OtherCandidate", &fonts).to_string();
    assert!(message.contains("first 4096 unique observed names"));
    assert_eq!(
        message
            .lines()
            .filter(|line| line.starts_with("  - "))
            .count(),
        3
    );
    assert!(message.len() < 1600);
    fonts.nerd_fonts.reverse();
    assert_eq!(not_found("OtherCandidate", &fonts).to_string(), message);
}
