use crate::{
    config::{
        auto_resolution::{self, ChoiceSource, FallbackReason},
        AutoConfig,
    },
    env::SlateEnv,
    theme::{ThemeAppearance, ThemeRegistry},
};
use serde::Serialize;

#[derive(Serialize)]
pub(crate) struct Report {
    dark: Entry,
    light: Entry,
}

impl Report {
    pub(crate) fn has_errors(&self) -> bool {
        self.dark.issue.is_some() || self.light.issue.is_some()
    }
}

#[derive(Serialize)]
struct Entry {
    requested_appearance: &'static str,
    status: &'static str,
    theme_id: Option<String>,
    name: Option<String>,
    theme_appearance: Option<&'static str>,
    source: Option<ChoiceSource>,
    #[serde(skip_serializing_if = "Option::is_none")]
    fallback_reason: Option<FallbackReason>,
    #[serde(skip_serializing_if = "Option::is_none")]
    issue: Option<&'static str>,
}

/// Capture the needed current-state observation at most once for both choices.
/// A malformed pairing document blocks both, as it does in runtime selection.
pub(crate) fn inspect(
    env: &SlateEnv,
    registry: &ThemeRegistry,
    pairing: Option<&AutoConfig>,
) -> Report {
    let current = pairing
        .filter(|pair| pair.dark_theme.is_none() || pair.light_theme.is_none())
        .map(|_| auto_resolution::read_current(env));
    let entry = |appearance| {
        let mut entry = Entry {
            requested_appearance: auto_resolution::appearance_name(appearance),
            status: "error",
            theme_id: None,
            name: None,
            theme_appearance: None,
            source: None,
            fallback_reason: None,
            issue: None,
        };
        let Some(pairing) = pairing else {
            entry.issue = Some(
                "Cannot read the pairing document safely; conditional selection is unavailable.",
            );
            return entry;
        };
        let uses_current = auto_resolution::configured(pairing, appearance).is_none();
        let choice =
            auto_resolution::choose(registry, pairing, appearance, || match current.as_ref() {
                Some(Ok(value)) => Ok(value.clone()),
                _ => Err(crate::error::SlateError::InvalidConfig(
                    "Current-theme tracking could not be inspected.".into(),
                )),
            });
        match choice {
            Ok(choice) => {
                entry.status = "resolved";
                entry.theme_id = Some(choice.theme.id.clone());
                entry.name = Some(choice.theme.name.clone());
                entry.theme_appearance =
                    Some(auto_resolution::appearance_name(choice.theme.appearance));
                entry.source = Some(choice.source);
                entry.fallback_reason = choice.fallback_reason;
            }
            Err(_) => {
                entry.issue = Some(
                    if uses_current && current.as_ref().is_some_and(|value| value.is_err()) {
                        "Cannot resolve the required current-theme tracking file safely; inspect this profile's current file, permissions, links, UTF-8 and 4 KiB limit. Contents omitted."
                    } else if !uses_current {
                        "The saved theme ID is not in the current catalog; it will not silently fall back. Contents omitted."
                    } else {
                        "The catalog cannot supply this automatic choice; inspect its pairing and built-in default definitions."
                    },
                )
            }
        }
        entry
    };
    Report {
        dark: entry(ThemeAppearance::Dark),
        light: entry(ThemeAppearance::Light),
    }
}

pub(crate) fn append_text(output: &mut String, report: &Report) {
    use std::fmt::Write;
    let _ = writeln!(output, "Automatic choices (conditional; no desktop query):");
    for entry in [&report.dark, &report.light] {
        if let (Some(id), Some(source)) = (&entry.theme_id, entry.source) {
            let _ = writeln!(
                output,
                "  {} -> {id} ({})",
                entry.requested_appearance,
                source.label()
            );
            if entry.theme_appearance != Some(entry.requested_appearance) {
                let _ = writeln!(output, "    Selected theme remains {}; explicit overrides and catalog self-pairs are preserved.", entry.theme_appearance.unwrap_or("unknown"));
            }
            if let Some(reason) = entry.fallback_reason {
                let reason = match reason {
                    FallbackReason::NoCurrentTheme => "No current theme is recorded.",
                    FallbackReason::UnknownCurrentTheme => {
                        "Recorded current theme is not in the catalog; contents omitted."
                    }
                    FallbackReason::NoCatalogPair => {
                        "The current theme has no catalog pair for this appearance."
                    }
                };
                let _ = writeln!(output, "    {reason}");
            }
        } else {
            let _ = writeln!(
                output,
                "  {}: error\n    {}",
                entry.requested_appearance,
                entry.issue.unwrap_or("Cannot resolve this appearance.")
            );
        }
    }
}
