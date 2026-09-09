//! One selection policy for runtime application and conditional inspection.
//! No desktop queries, writes, output or brand events occur here.
use super::{
    file_read::{self, Links, MAX_STATE_BYTES},
    pairing, recovery_paths, AutoConfig,
};
use crate::{
    env::SlateEnv,
    error::{Result, SlateError},
    theme::{ThemeAppearance, ThemeRegistry, ThemeVariant},
};
use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ChoiceSource {
    Configured,
    CurrentTheme,
    CatalogPair,
    BrandDefault,
}

impl ChoiceSource {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Configured => "saved pairing",
            Self::CurrentTheme => "current theme",
            Self::CatalogPair => "catalog pairing",
            Self::BrandDefault => "built-in default",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum FallbackReason {
    NoCurrentTheme,
    UnknownCurrentTheme,
    NoCatalogPair,
}

pub(crate) struct Choice<'a> {
    pub(crate) theme: &'a ThemeVariant,
    pub(crate) source: ChoiceSource,
    pub(crate) fallback_reason: Option<FallbackReason>,
}

pub(crate) fn configured(pairing: &AutoConfig, appearance: ThemeAppearance) -> Option<&str> {
    match appearance {
        ThemeAppearance::Dark => pairing.dark_theme.as_deref(),
        ThemeAppearance::Light => pairing.light_theme.as_deref(),
    }
}

/// Like tracked-state reading, blank content means unset. Reject unsafe paths
/// with the same read contract as pairing inspection; never echo stored IDs.
pub(crate) fn read_current(env: &SlateEnv) -> Result<Option<String>> {
    let path = env.managed_file("current");
    recovery_paths::validate_file_path(env, &path, "automatic theme selection")?;
    let source = file_read::read(&path, MAX_STATE_BYTES, Links::Reject).map_err(|_| {
        SlateError::InvalidConfig(
            "Cannot safely read current-theme tracking (4 KiB limit); contents omitted.".into(),
        )
    })?;
    source
        .map(|source| {
            std::str::from_utf8(&source.bytes)
                .map(|text| {
                    let text = text.trim();
                    (!text.is_empty()).then(|| text.to_owned())
                })
                .map_err(|_| {
                    SlateError::InvalidConfig(
                        "Current-theme tracking is not UTF-8; contents omitted.".into(),
                    )
                })
        })
        .transpose()
        .map(Option::flatten)
}

/// The current-theme closure is lazy: an explicit pairing never depends on an
/// unrelated current-state file. Known stored overrides may cross appearance;
/// preserve that existing behavior and report the actual selected appearance.
pub(crate) fn choose<'a>(
    registry: &'a ThemeRegistry,
    pairing: &AutoConfig,
    appearance: ThemeAppearance,
    current: impl FnOnce() -> Result<Option<String>>,
) -> Result<Choice<'a>> {
    if let Some(id) = configured(pairing, appearance) {
        let theme = registry.get(id).ok_or_else(|| SlateError::InvalidConfig(format!(
            "Saved {} auto-theme pairing is not in the current catalog; contents omitted. Inspect `slate config pairing --json` and replace that slot with a valid theme ID.",
            appearance_name(appearance)
        )))?;
        return Ok(Choice {
            theme,
            source: ChoiceSource::Configured,
            fallback_reason: None,
        });
    }
    let current = current()?;
    let fallback_reason = match current.as_deref() {
        Some(id) => match registry.get(id) {
            Some(theme) => {
                if theme.appearance == appearance {
                    return Ok(Choice {
                        theme,
                        source: ChoiceSource::CurrentTheme,
                        fallback_reason: None,
                    });
                }
                if let Some(pair) = theme.auto_pair.as_deref() {
                    let theme = registry.get(pair).ok_or_else(|| {
                        SlateError::InvalidThemeData(
                            "Catalog automatic pairing is missing its target.".into(),
                        )
                    })?;
                    return Ok(Choice {
                        theme,
                        source: ChoiceSource::CatalogPair,
                        fallback_reason: None,
                    });
                }
                FallbackReason::NoCatalogPair
            }
            None => FallbackReason::UnknownCurrentTheme,
        },
        None => FallbackReason::NoCurrentTheme,
    };
    let id = match appearance {
        ThemeAppearance::Dark => "catppuccin-mocha",
        ThemeAppearance::Light => "catppuccin-latte",
    };
    let theme = registry.get(id).ok_or_else(|| {
        SlateError::InvalidThemeData("Catalog automatic theme default is unavailable.".into())
    })?;
    Ok(Choice {
        theme,
        source: ChoiceSource::BrandDefault,
        fallback_reason: Some(fallback_reason),
    })
}

pub(crate) fn appearance_name(appearance: ThemeAppearance) -> &'static str {
    match appearance {
        ThemeAppearance::Dark => "dark",
        ThemeAppearance::Light => "light",
    }
}

pub(crate) fn resolve<'a>(
    env: &SlateEnv,
    registry: &'a ThemeRegistry,
    appearance: ThemeAppearance,
) -> Result<Choice<'a>> {
    let pairing = pairing::inspect(env)?;
    choose(registry, &pairing, appearance, || read_current(env))
}

#[cfg(test)]
mod tests;
