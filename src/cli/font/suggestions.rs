//! Advisory exact-family hints from an already captured discovery result.
//! No discovery, installation, native probing or fuzzy selection occurs here.
use super::{choices::terminal_text, FontCatalog};
use crate::{
    adapter::{
        font::{FontAdapter, FontDiscovery},
        font_config,
    },
    error::SlateError,
    lookup::close_distance,
};
use std::collections::BTreeSet;

const MAX_SUGGESTIONS: usize = 3;
const MAX_OBSERVED: usize = 4096;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Source {
    Observed,
    Catalog,
}
impl Source {
    fn label(self) -> &'static str {
        match self {
            Self::Observed => "observed candidate",
            Self::Catalog => "catalog choice; may download",
        }
    }
}

struct Suggestion<'a> {
    family: &'a str,
    source: Source,
}

fn rank(query: &str, chars: &[char], key: &str) -> Option<(u8, usize)> {
    if key.starts_with(query) {
        return Some((0, key.chars().count().saturating_sub(chars.len())));
    }
    if chars.len() >= 4 && key.contains(query) {
        return Some((1, key.chars().count().saturating_sub(chars.len())));
    }
    let threshold = match chars.len() {
        0..=4 => 1,
        5..=11 => 2,
        _ => 3,
    };
    close_distance(chars, key, threshold).map(|distance| (2, distance))
}

fn nearby<'a>(query: &str, discovery: &'a FontDiscovery) -> (Vec<Suggestion<'a>>, bool) {
    if query.chars().take(65).count() > 64 {
        return (Vec::new(), false);
    }
    let key = FontAdapter::family_match_key(query);
    let chars: Vec<_> = key.chars().take(65).collect();
    if !(3..=64).contains(&chars.len()) {
        return (Vec::new(), false);
    }
    let observed: BTreeSet<_> = discovery
        .nerd_fonts
        .iter()
        .chain(&discovery.system_fonts)
        .filter(|name| font_config::validate_family(name).is_ok())
        .map(String::as_str)
        .collect();
    let limited = observed.len() > MAX_OBSERVED;
    let mut matches = Vec::new();
    for family in observed.iter().copied().take(MAX_OBSERVED) {
        let full = FontAdapter::family_match_key(family);
        let short = full.strip_suffix("nerdfont").unwrap_or(&full);
        if let Some(score) = [full.as_str(), short]
            .into_iter()
            .filter_map(|name| rank(&key, &chars, name))
            .min()
        {
            matches.push((score, Source::Observed, family));
        }
    }
    for font in FontCatalog::all_fonts() {
        let full = FontAdapter::family_match_key(font.name);
        // Show exact observed names instead of a catalog spelling that could
        // resolve ambiguously. Inspect the full set, even when scoring is capped.
        if observed
            .iter()
            .any(|name| FontAdapter::family_match_key(name) == full)
        {
            continue;
        }
        if let Some(score) = [full, FontAdapter::family_match_key(font.id)]
            .iter()
            .filter_map(|name| rank(&key, &chars, name))
            .min()
        {
            matches.push((score, Source::Catalog, font.name));
        }
    }
    matches.sort_unstable();
    (
        matches
            .into_iter()
            .take(MAX_SUGGESTIONS)
            .map(|(_, source, family)| Suggestion { family, source })
            .collect(),
        limited,
    )
}

pub(super) fn not_found(name: &str, discovery: &FontDiscovery) -> SlateError {
    let mut message = format!(
        "Font '{}' not found. Run 'slate font --list' to see available options.",
        terminal_text(name)
    );
    let (suggestions, limited) = nearby(name, discovery);
    if !suggestions.is_empty() {
        message.push_str("\nNearby exact family names (display-escaped):");
        for suggestion in suggestions {
            message.push_str(&format!(
                "\n  - {:?} ({})",
                suggestion.family,
                suggestion.source.label()
            ));
        }
        message.push_str("\nUse an exact name with `slate font <name> --dry-run` to inspect it. Suggestions do not select or install a font; native availability is unverified.");
    }
    if limited {
        message.push_str(&format!("\nSuggestions scored only the first {MAX_OBSERVED} unique observed names; `slate font --list` retains the full captured inventory."));
    }
    SlateError::InvalidConfig(message)
}

pub(super) fn ambiguous<'a>(names: impl IntoIterator<Item = &'a str>) -> SlateError {
    let mut names = names.into_iter();
    let mut message = "Several font candidates match this name. Use an exact family name from `slate font --list` instead of an ambiguous alias.\nMatching exact family names (display-escaped):".to_string();
    for name in names.by_ref().take(MAX_SUGGESTIONS) {
        message.push_str(&format!("\n  - {name:?}"));
    }
    let remaining = names.count();
    if remaining > 0 {
        message.push_str(&format!(
            "\n  ... and {remaining} more; inspect `slate font --list`."
        ));
    }
    message.push_str(
        "\nNo font was selected from these candidates; native availability is unverified.",
    );
    SlateError::InvalidConfig(message)
}

#[cfg(test)]
#[path = "suggestions_tests.rs"]
mod tests;
