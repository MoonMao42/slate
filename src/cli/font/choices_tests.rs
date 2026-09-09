use super::*;
use crate::adapter::font::FontDiscovery;

#[test]
fn saved_font_selection_requires_exact_or_unique_installed_family() {
    let items = Choices::from_scan(&scan(
        &[
            "Twin-Mono Nerd Font",
            "Twin Mono Nerd Font",
            "JetBrainsMono Nerd Font",
        ],
        true,
    ))
    .picker_items();
    let key = saved_key(&items, "Twin-Mono Nerd Font").unwrap();
    assert_eq!(
        items.iter().find(|item| item.key == key).unwrap().family,
        "Twin-Mono Nerd Font"
    );
    assert!(
        saved_key(&items, "TwinMono Nerd Font").is_none(),
        "ambiguous aliases must not select arbitrarily"
    );
    assert!(saved_key(&items, "JetBrains Mono Nerd Font").is_some());
    assert!(saved_key(&items, "Missing Nerd Font").is_none());
    assert!(items
        .iter()
        .any(|item| item.needs_install && item.family == "Hack Nerd Font"));
    assert!(
        saved_key(&items, "Hack Nerd Font").is_none(),
        "never select an install offer as a saved installed font"
    );
    assert!(saved_key(&items, "✦ JetBrainsMono Nerd Font (recommended)").is_none());
}

fn scan(names: &[&str], complete: bool) -> FontScanReport {
    FontScanReport {
        fonts: FontDiscovery {
            nerd_fonts: names.iter().map(|name| (*name).into()).collect(),
            system_fonts: vec!["Menlo".into()],
        },
        omitted_issues: usize::from(!complete),
        ..Default::default()
    }
}

#[test]
fn font_list_choices_preserve_all_variants_and_exact_names_with_unique_picker_keys() {
    let scan = scan(
        &[
            "JetBrainsMono Nerd Font Mono",
            "JetBrainsMono Nerd Font",
            "JetBrainsMono Nerd Font Propo",
            "Twin-Mono Nerd Font",
            "Twin Mono Nerd Font",
            "JetBrainsMono Nerd Font",
            "bad\nfont",
        ],
        true,
    );
    let choices = Choices::from_scan(&scan);
    assert_eq!(choices.candidates.len(), 6);
    assert_eq!(choices.candidates[0].family, "JetBrainsMono Nerd Font");
    assert!(choices.candidates[0].recommended);
    let items = choices.picker_items();
    let keys: std::collections::BTreeSet<_> = items.iter().map(|item| &item.key).collect();
    assert_eq!(keys.len(), items.len());
    for candidate in &choices.candidates {
        assert_eq!(
            items
                .iter()
                .filter(|item| !item.needs_install && item.family == candidate.family)
                .count(),
            1
        );
        assert!(super::super::resolve_font_choice_with_scan(&candidate.family, &scan).is_ok());
    }
}

#[test]
fn font_list_catalog_preserves_partial_unknown_and_multiple_positive_matches() {
    for complete in [true, false] {
        let choices = Choices::from_scan(&scan(
            &["JetBrainsMono Nerd Font", "JetBrains-Mono Nerd Font"],
            complete,
        ));
        let entry = choices
            .catalog
            .iter()
            .find(|entry| entry.id == "jetbrains-mono")
            .unwrap();
        assert_eq!(entry.presence, Presence::CandidateFound);
        assert_eq!(entry.matching_candidates.len(), 2);
        assert!(!entry.download_offered);
        for other in choices
            .catalog
            .iter()
            .filter(|entry| entry.id != "jetbrains-mono")
        {
            assert_eq!(
                other.presence,
                if complete {
                    Presence::NotObserved
                } else {
                    Presence::Unknown
                }
            );
            assert_eq!(other.download_offered, complete);
        }
        assert_eq!(
            choices
                .picker_items()
                .iter()
                .filter(|item| item.needs_install)
                .count(),
            if complete { 3 } else { 0 }
        );
    }
}

#[test]
fn font_list_display_labels_never_replace_or_strip_family_data() {
    let names = [
        "✦ Literal Nerd Font (recommended)",
        "Literal Nerd Font (not installed)",
        "字形 Nerd Font\u{202e}",
        "Literal Nerd Font (download)",
    ];
    let choices = Choices::from_scan(&scan(&names, false));
    let items = choices.picker_items();
    for family in names {
        let item = items.iter().find(|item| item.family == family).unwrap();
        assert!(!item.needs_install);
        assert!(!item.label.contains('\u{202e}'));
        assert_eq!(item.label, terminal_text(family));
    }
    assert_eq!(terminal_text("\u{1b}[2J"), "\\u{1b}[2J");
}
