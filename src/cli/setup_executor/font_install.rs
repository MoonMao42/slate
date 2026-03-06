use crate::adapter::font::FontAdapter;
use crate::cli::font_selection::FontCatalog;
use crate::detection;
use crate::env::SlateEnv;
use crate::error::Result;
use std::path::{Path, PathBuf};
use std::process::Command;

pub(crate) fn planned_font_installs(selected_font: Option<&str>) -> Vec<String> {
    match selected_font {
        Some(font) => vec![font.to_string()],
        None => vec![],
    }
}

/// Copy font files from Homebrew Caskroom to the current user's font directory.
pub fn copy_font_from_caskroom(font_name_or_id: &str, env: &SlateEnv) -> Result<()> {
    use std::fs;

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

    let font_files: Vec<PathBuf> = walkdir(&caskroom, &["ttf", "otf", "ttc"]);
    if font_files.is_empty() {
        return Err(crate::error::SlateError::Internal(
            "No font files found in Caskroom".to_string(),
        ));
    }

    let home = dirs_font_target(env);
    fs::create_dir_all(&home)?;

    for src in &font_files {
        if let Some(filename) = src.file_name() {
            let dest = home.join(filename);
            fs::copy(src, &dest)?;
        }
    }

    let _ = crate::platform::fonts::refresh_font_cache();
    Ok(())
}

pub fn download_font_release(font_name_or_id: &str, env: &SlateEnv) -> Result<()> {
    const NERD_FONTS_RELEASE_BASE: &str =
        "https://github.com/ryanoasis/nerd-fonts/releases/latest/download";

    let font = FontCatalog::get_font(font_name_or_id).ok_or_else(|| {
        crate::error::SlateError::Internal(format!("Unknown font: {}", font_name_or_id))
    })?;

    let temp = create_writable_temp_dir(env).map_err(|e| {
        crate::error::SlateError::Internal(format!(
            "Cannot create temporary directory for font download \
             (tried $TMPDIR, /tmp, ~/.cache/slate/tmp): {}",
            e
        ))
    })?;

    let archive = temp.path().join(format!("{}.zip", font.release_asset));
    let extract_dir = temp.path().join("extract");
    std::fs::create_dir_all(&extract_dir).map_err(|e| {
        crate::error::SlateError::Internal(format!(
            "Cannot create extraction directory {}: {}",
            extract_dir.display(),
            e
        ))
    })?;

    let url = format!("{}/{}.zip", NERD_FONTS_RELEASE_BASE, font.release_asset);
    let curl = detection::command_path("curl").ok_or_else(|| {
        crate::error::SlateError::Internal(
            "curl was not found. Install curl, then rerun slate setup.".to_string(),
        )
    })?;
    let download_output = Command::new(curl)
        .arg("-fsSL")
        .arg("--connect-timeout")
        .arg("10")
        .arg("--max-time")
        .arg("90")
        .arg("--http1.1")
        .arg("--retry")
        .arg("2")
        .arg("--retry-all-errors")
        .arg("-A")
        .arg("slate-font-bootstrap")
        .arg("-o")
        .arg(&archive)
        .arg(&url)
        .output()
        .map_err(|err| {
            crate::error::SlateError::Internal(format!(
                "Failed to download font archive from Nerd Fonts releases: {}",
                err
            ))
        })?;

    if !download_output.status.success() {
        let stderr = String::from_utf8_lossy(&download_output.stderr);
        let stdout = String::from_utf8_lossy(&download_output.stdout);
        return Err(crate::error::SlateError::Internal(format!(
            "Font release download failed: {}",
            super::tool_install::first_meaningful_command_line(&stderr, &stdout)
        )));
    }

    let unzip = detection::command_path("unzip").ok_or_else(|| {
        crate::error::SlateError::Internal(
            "unzip was not found. Install unzip, then rerun slate setup.".to_string(),
        )
    })?;
    let unzip_output = Command::new(unzip)
        .arg("-oq")
        .arg(&archive)
        .arg("-d")
        .arg(&extract_dir)
        .output()
        .map_err(|err| {
            crate::error::SlateError::Internal(format!(
                "Failed to extract downloaded font archive: {}",
                err
            ))
        })?;

    if !unzip_output.status.success() {
        let stderr = String::from_utf8_lossy(&unzip_output.stderr);
        let stdout = String::from_utf8_lossy(&unzip_output.stdout);
        return Err(crate::error::SlateError::Internal(format!(
            "Font archive extraction failed: {}",
            super::tool_install::first_meaningful_command_line(&stderr, &stdout)
        )));
    }

    let font_files = walkdir(&extract_dir, &["ttf", "otf", "ttc"]);
    if font_files.is_empty() {
        return Err(crate::error::SlateError::Internal(
            "Downloaded font archive did not contain any font files".to_string(),
        ));
    }

    let target = dirs_font_target(env);
    std::fs::create_dir_all(&target).map_err(|e| {
        crate::error::SlateError::Internal(format!(
            "Cannot create font directory {}: {}",
            target.display(),
            e
        ))
    })?;
    for src in &font_files {
        if let Some(filename) = src.file_name() {
            std::fs::copy(src, target.join(filename)).map_err(|e| {
                crate::error::SlateError::Internal(format!(
                    "Cannot install font file {} to {}: {}",
                    filename.to_string_lossy(),
                    target.display(),
                    e
                ))
            })?;
        }
    }

    let _ = crate::platform::fonts::refresh_font_cache();
    Ok(())
}

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
    let mut cmd = Command::new(brew);
    detection::apply_normalized_path(&mut cmd);
    cmd.arg("install").arg("--cask").arg(cask_name);

    let output = cmd.output().map_err(|e| {
        crate::error::SlateError::Internal(format!("Failed to execute brew: {}", e))
    })?;

    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(crate::error::SlateError::Internal(
            super::tool_install::classify_brew_error(cask_name, &stderr),
        ))
    }
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

pub(crate) fn is_font_installed_with_env(env: &SlateEnv, font_name_or_id: &str) -> bool {
    if let Ok(installed) = FontAdapter::detect_installed_fonts_with_env(env) {
        let lookup = FontCatalog::get_font(font_name_or_id)
            .map(|font| font.name.to_string())
            .unwrap_or_else(|| font_name_or_id.to_string());
        let lookup_key = FontAdapter::family_match_key(&lookup);
        installed
            .iter()
            .any(|family| FontAdapter::family_match_key(family) == lookup_key)
    } else {
        false
    }
}

pub(crate) fn resolve_font_family_with_env(env: &SlateEnv, font_name_or_id: &str) -> String {
    if let Some(font) = FontCatalog::get_font(font_name_or_id) {
        if let Ok(installed) = FontAdapter::detect_installed_fonts_with_env(env) {
            let catalog_key = FontAdapter::family_match_key(font.name);
            if let Some(matched) = installed
                .iter()
                .find(|family| FontAdapter::family_match_key(family) == catalog_key)
            {
                return matched.clone();
            }
        }
        return font.name.to_string();
    }
    font_name_or_id.to_string()
}

fn walkdir(dir: &Path, exts: &[&str]) -> Vec<PathBuf> {
    let mut results = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                results.extend(walkdir(&path, exts));
            } else if path
                .extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| {
                    exts.iter()
                        .any(|candidate| ext.eq_ignore_ascii_case(candidate))
                })
            {
                results.push(path);
            }
        }
    }
    results
}

fn dirs_font_target(env: &SlateEnv) -> PathBuf {
    crate::platform::fonts::user_font_dir(env)
}
