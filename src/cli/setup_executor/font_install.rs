use crate::adapter::font::FontAdapter;
use crate::cli::font_selection::FontCatalog;
use crate::detection;
use crate::env::SlateEnv;
use crate::error::Result;
use crate::platform::fonts::FontCacheRefresh;
use std::path::PathBuf;

mod archive;
pub(crate) mod chain;
mod download;
mod files;

/// Copy font files from Homebrew Caskroom to the current user's font directory.
pub fn copy_font_from_caskroom(font_name_or_id: &str, env: &SlateEnv) -> Result<FontCacheRefresh> {
    let cask_name = FontCatalog::get_font(font_name_or_id)
        .map(|font| font.brew_cask)
        .ok_or_else(|| {
            crate::error::SlateError::Internal(format!("Unknown font: {}", font_name_or_id))
        })?;

    let caskroom = detection::homebrew_prefix()
        .map(|prefix| prefix.join("Caskroom").join(cask_name))
        .unwrap_or_else(|| PathBuf::from("/opt/homebrew/Caskroom").join(cask_name));
    if !caskroom.exists() {
        return Err(crate::error::SlateError::Internal(
            "Font not found in Homebrew Caskroom".to_string(),
        ));
    }

    finish_file_install(files::install(&caskroom, env), || {
        crate::platform::fonts::refresh_font_cache(env)
    })
}

pub fn download_font_release(font_name_or_id: &str, env: &SlateEnv) -> Result<FontCacheRefresh> {
    let font = FontCatalog::get_font(font_name_or_id).ok_or_else(|| {
        crate::error::SlateError::Internal(format!("Unknown font: {}", font_name_or_id))
    })?;

    let curl = detection::command_in_actual_path("curl")
        .or_else(|| detection::command_path("curl"))
        .ok_or_else(|| {
            crate::error::SlateError::Internal(
                "curl was not found. Install curl, then rerun slate setup.".to_string(),
            )
        })?;
    finish_file_install(download::install(font.release_asset, &curl, env), || {
        crate::platform::fonts::refresh_font_cache(env)
    })
}

fn finish_file_install(
    files: Result<files::Report>,
    refresh: impl FnOnce() -> FontCacheRefresh,
) -> Result<FontCacheRefresh> {
    files?;
    // A cache warning is still successful file installation. Returning Err here
    // would incorrectly trigger another download or invite deleting valid fonts.
    Ok(refresh())
}

#[cfg(test)]
#[path = "font_install/cache_tests.rs"]
mod cache_tests;

pub fn install_font(font_name_or_id: &str) -> Result<()> {
    if !matches!(
        crate::platform::packages::detect_backend(),
        crate::platform::packages::PackageManagerBackend::Homebrew
    ) {
        return Err(crate::error::SlateError::Internal(
            "Homebrew font installation is only used on macOS. Slate will use direct Nerd Fonts downloads on Linux.".to_string(),
        ));
    }

    let cask_name = FontCatalog::get_font(font_name_or_id)
        .map(|font| font.brew_cask)
        .or_else(|| {
            FontCatalog::all_fonts()
                .into_iter()
                .find(|font| {
                    font.name == font_name_or_id
                        || font.name.replace(" Nerd Font", "") == font_name_or_id
                })
                .map(|font| font.brew_cask)
        })
        .ok_or_else(|| {
            crate::error::SlateError::Internal(format!("Unknown font: {}", font_name_or_id))
        })?;

    let brew = detection::homebrew_executable().ok_or_else(|| {
        crate::error::SlateError::Internal(
            "Homebrew was not found. Install it first or add it to PATH.".to_string(),
        )
    })?;
    super::homebrew::install(
        &brew,
        cask_name,
        crate::cli::tool_selection::BrewKind::Cask,
        super::homebrew::FONT_LIMITS,
    )
}

pub(super) fn create_writable_temp_dir(env: &SlateEnv) -> std::io::Result<tempfile::TempDir> {
    use tempfile::TempDir;

    TempDir::new()
        .or_else(|_| TempDir::new_in("/tmp"))
        .or_else(|_| {
            let fallback = env.slate_cache_dir().join("tmp");
            std::fs::create_dir_all(&fallback)?;
            TempDir::new_in(&fallback)
        })
}

pub(crate) fn font_display_name(font_name_or_id: &str) -> String {
    FontCatalog::get_font(font_name_or_id)
        .map(|font| font.name.to_string())
        .unwrap_or_else(|| font_name_or_id.to_string())
}

pub(crate) fn strip_error_prefix(msg: &str) -> &str {
    msg.strip_prefix("Internal error: ")
        .or_else(|| msg.strip_prefix("IO error: "))
        .unwrap_or(msg)
}

pub(crate) fn is_font_installed_with_env(env: &SlateEnv, font_name_or_id: &str) -> Result<bool> {
    let lookup = FontCatalog::get_font(font_name_or_id)
        .map(|font| font.name.to_string())
        .unwrap_or_else(|| font_name_or_id.to_string());
    FontAdapter::scan_fonts_with_env(env).contains_nerd_family(&lookup)
}

pub(crate) fn resolve_font_family_with_env(env: &SlateEnv, font_name_or_id: &str) -> String {
    if let Some(font) = FontCatalog::get_font(font_name_or_id) {
        let report = FontAdapter::scan_fonts_with_env(env);
        let catalog_key = FontAdapter::family_match_key(font.name);
        if let Some(matched) = report
            .fonts
            .nerd_fonts
            .iter()
            .find(|family| FontAdapter::family_match_key(family) == catalog_key)
        {
            return matched.clone();
        }
        return font.name.to_string();
    }
    font_name_or_id.to_string()
}
