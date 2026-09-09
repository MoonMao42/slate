//! Shared, non-mutating candidate model for the list and interactive picker.
//! Keep family data separate from decorated/escaped display labels.
use crate::adapter::{
    font::{FontAdapter, FontScanReport},
    font_config,
};
use crate::cli::font_selection::FontCatalog;
use crate::cli::ui_language::tr;
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Kind {
    Nerd,
    System,
}

#[derive(Debug, Serialize)]
pub(super) struct Candidate {
    pub family: String,
    pub kind: Kind,
    pub recommended: bool,
}

#[derive(Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Presence {
    CandidateFound,
    NotObserved,
    Unknown,
}

#[derive(Debug, Serialize)]
pub(super) struct CatalogEntry {
    pub id: &'static str,
    pub family: &'static str,
    pub presence: Presence,
    pub matching_candidates: Vec<String>,
    pub download_offered: bool,
}

#[derive(Debug, Serialize)]
pub(super) struct Choices {
    pub candidates: Vec<Candidate>,
    pub catalog: Vec<CatalogEntry>,
}

pub(super) struct PickerItem {
    pub key: String,
    pub label: String,
    pub family: String,
    pub needs_install: bool,
    pub hint: &'static str,
}

/// Saved family data must never match a display badge or a download offer.
pub(super) fn saved_key<'a>(items: &'a [PickerItem], saved: &str) -> Option<&'a str> {
    if let Some(item) = items
        .iter()
        .find(|item| !item.needs_install && item.family == saved)
    {
        return Some(&item.key);
    }
    let key = FontAdapter::family_match_key(saved);
    let mut matches = items
        .iter()
        .filter(|item| !item.needs_install && FontAdapter::family_match_key(&item.family) == key);
    let first = matches.next()?;
    matches.next().is_none().then_some(first.key.as_str())
}

impl Choices {
    pub fn from_scan(scan: &FontScanReport) -> Self {
        let mut unique = BTreeMap::new();
        for (kind, names) in [
            (Kind::Nerd, &scan.fonts.nerd_fonts),
            (Kind::System, &scan.fonts.system_fonts),
        ] {
            for family in names {
                if font_config::validate_family(family).is_ok() {
                    unique.entry(family.clone()).or_insert(kind);
                }
            }
        }
        let mut candidates: Vec<_> = unique
            .into_iter()
            .map(|(family, kind)| Candidate {
                recommended: family == "JetBrainsMono Nerd Font",
                family,
                kind,
            })
            .collect();
        candidates.sort_by(|a, b| {
            (a.kind, !a.recommended, &a.family).cmp(&(b.kind, !b.recommended, &b.family))
        });
        let catalog = FontCatalog::all_fonts()
            .into_iter()
            .map(|entry| {
                let key = FontAdapter::family_match_key(entry.name);
                let matching_candidates: Vec<_> = candidates
                    .iter()
                    .filter(|candidate| FontAdapter::family_match_key(&candidate.family) == key)
                    .map(|candidate| candidate.family.clone())
                    .collect();
                let presence = if !matching_candidates.is_empty() {
                    Presence::CandidateFound
                } else if scan.is_complete() {
                    Presence::NotObserved
                } else {
                    Presence::Unknown
                };
                let download_offered = presence == Presence::NotObserved;
                CatalogEntry {
                    id: entry.id,
                    family: entry.name,
                    presence,
                    matching_candidates,
                    download_offered,
                }
            })
            .collect();
        Self {
            candidates,
            catalog,
        }
    }

    pub fn picker_items(&self) -> Vec<PickerItem> {
        let mut items = Vec::new();
        for kind in [Kind::Nerd, Kind::System] {
            if kind == Kind::System {
                for (index, entry) in self
                    .catalog
                    .iter()
                    .filter(|entry| entry.download_offered)
                    .enumerate()
                {
                    items.push(PickerItem {
                        key: format!("catalog_{index}"),
                        label: format!("{} ({})", entry.family, tr("下载", "download")),
                        family: entry.family.into(),
                        needs_install: true,
                        hint: if index == 0 {
                            tr("可下载字体", "Catalog Downloads")
                        } else {
                            ""
                        },
                    });
                }
            }
            for (index, candidate) in self
                .candidates
                .iter()
                .filter(|candidate| candidate.kind == kind)
                .enumerate()
            {
                let prefix = if kind == Kind::Nerd { "nerd" } else { "system" };
                let name = terminal_text(&candidate.family);
                items.push(PickerItem {
                    key: format!("{prefix}_{index}"),
                    label: if candidate.recommended {
                        format!("✦ {name} ({})", tr("推荐", "recommended"))
                    } else {
                        name
                    },
                    family: candidate.family.clone(),
                    needs_install: false,
                    hint: if index != 0 {
                        ""
                    } else if kind == Kind::Nerd {
                        tr("检测到的 Nerd 字体", "Nerd Font Candidates")
                    } else {
                        tr("检测到的系统字体", "System Candidates")
                    },
                });
            }
        }
        items
    }
}

pub(super) fn terminal_text(text: &str) -> String {
    let mut output = String::new();
    for c in text.chars() {
        if c.is_control() || matches!(c, '\u{2028}'..='\u{202e}' | '\u{2066}'..='\u{2069}') {
            output.extend(c.escape_default());
        } else {
            output.push(c);
        }
    }
    output
}

#[cfg(test)]
#[path = "choices_tests.rs"]
mod tests;
