//! One ordered fallback policy for setup, explicit selection, picker and import.
//! Successful file publication (even with cache warnings) ends installation.
use super::*;
use crate::error::SlateError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Stage {
    Homebrew,
    SharedCache,
    Download,
}

impl Stage {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Homebrew => "Homebrew",
            Self::SharedCache => "shared font cache",
            Self::Download => "direct font download",
        }
    }
}

#[derive(Debug)]
pub(crate) struct Report {
    pub stage: Stage,
    pub cache: FontCacheRefresh,
    /// Known failed attempts preceding success, not fatal installation errors.
    pub notices: Vec<String>,
}

pub(crate) fn install_catalog(
    request: &str,
    env: &SlateEnv,
    progress: impl FnMut(Stage),
) -> Result<Report> {
    let font = FontCatalog::all_fonts()
        .into_iter()
        .find(|font| request == font.id || request == font.name)
        .ok_or_else(|| {
            SlateError::InvalidConfig(format!(
                "No automatic installation source for font '{}'; install it manually and retry",
                request.escape_default()
            ))
        })?;
    let homebrew = matches!(
        crate::platform::packages::detect_backend(),
        crate::platform::packages::PackageManagerBackend::Homebrew
    );
    try_install(
        homebrew,
        || install_font(font.id).map(|()| FontCacheRefresh::NotNeeded),
        || copy_font_from_caskroom(font.id, env),
        || download_font_release(font.id, env),
        progress,
    )
}

pub(in crate::cli::setup_executor) fn try_install(
    homebrew: bool,
    brew: impl FnOnce() -> Result<FontCacheRefresh>,
    caskroom: impl FnOnce() -> Result<FontCacheRefresh>,
    download: impl FnOnce() -> Result<FontCacheRefresh>,
    mut progress: impl FnMut(Stage),
) -> Result<Report> {
    let mut failures = Vec::new();
    let mut consider = |stage, result| match result {
        Ok(cache) => Ok(Some(Report {
            stage,
            cache,
            notices: std::mem::take(&mut failures),
        })),
        Err(error) if !super::super::installation_fallback_allowed(&error) => Err(error),
        Err(error) => {
            failures.push(format!("{}: {error}", stage.label()));
            Ok(None)
        }
    };
    if homebrew {
        progress(Stage::Homebrew);
        if let Some(report) = consider(Stage::Homebrew, brew())? {
            return Ok(report);
        }
        progress(Stage::SharedCache);
        if let Some(report) = consider(Stage::SharedCache, caskroom())? {
            return Ok(report);
        }
    }
    progress(Stage::Download);
    match download() {
        Ok(cache) => Ok(Report {
            stage: Stage::Download,
            cache,
            notices: failures,
        }),
        Err(error)
            if failures.is_empty() || !super::super::installation_fallback_allowed(&error) =>
        {
            Err(error)
        }
        Err(error) => {
            failures.push(format!("{}: {error}", Stage::Download.label()));
            Err(SlateError::Internal(format!(
                "Font installation attempts failed: {}",
                failures.join("; ")
            )))
        }
    }
}

#[cfg(test)]
mod tests;
