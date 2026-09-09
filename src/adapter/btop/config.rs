//! Lossless single-value editing. btop's syntax is not TOML and its quoted
//! strings do not interpret escapes. Reject ambiguity instead of reserializing.
use super::{palette, BtopAdapter};
use crate::{
    config::{
        atomic_write_synced,
        file_read::{self, Links, Source, MAX_TOOL_CONFIG_BYTES},
        recovery_paths,
    },
    env::SlateEnv,
    error::{Result, SlateError},
    theme::ThemeVariant,
};
use std::{ops::Range, path::PathBuf};

fn invalid(reason: &str) -> SlateError {
    SlateError::InvalidConfig(format!("btop: {reason}; file contents omitted"))
}

fn literal_path(env: &SlateEnv) -> Result<String> {
    let path = BtopAdapter::theme_path(env);
    let value = path
        .to_str()
        .ok_or_else(|| invalid("theme path is not UTF-8"))?;
    if !path.is_absolute() || value.contains('"') || value.chars().any(char::is_control) {
        return Err(invalid(
            "theme path cannot be represented in btop configuration",
        ));
    }
    Ok(value.to_owned())
}

struct Binding<'a> {
    value: &'a str,
    range: Range<usize>,
}

fn binding(text: &str) -> Result<Option<Binding<'_>>> {
    if text.len() as u64 > MAX_TOOL_CONFIG_BYTES || text.contains('\0') {
        return Err(invalid("invalid or oversized configuration"));
    }
    let mut found = None;
    let mut offset = 0;
    for raw in text.split_inclusive('\n') {
        let line = raw.trim_end_matches(['\n', '\r']);
        let trimmed = line.trim_start_matches([' ', '\t']);
        if !trimmed.is_empty() && !trimmed.starts_with('#') {
            let (key, rest) = trimmed
                .split_once('=')
                .ok_or_else(|| invalid("expected a single-line key=value assignment"))?;
            let key = key.trim_end_matches([' ', '\t']);
            if key.is_empty() || !key.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_') {
                return Err(invalid("invalid assignment name"));
            }
            let value = rest.trim_start_matches([' ', '\t']);
            let (inside, len) = if let Some(quoted) = value.strip_prefix('"') {
                let end = quoted
                    .find('"')
                    .ok_or_else(|| invalid("unterminated quoted value"))?;
                (&quoted[..end], end + 2)
            } else {
                let end = value.find(char::is_whitespace).unwrap_or(value.len());
                if end == 0 || value[..end].contains('"') || value.starts_with('#') {
                    return Err(invalid("missing or ambiguous value"));
                }
                (&value[..end], end)
            };
            let tail = value[len..].trim_start_matches([' ', '\t']);
            if !tail.is_empty() && !tail.starts_with('#') {
                return Err(invalid("ambiguous text after assignment"));
            }
            if key == "color_theme" {
                if found.is_some() {
                    return Err(invalid("duplicate color_theme assignments"));
                }
                let start = offset + line.len() - value.len();
                found = Some(Binding {
                    value: inside,
                    range: start..start + len,
                });
            }
        }
        offset += raw.len();
    }
    Ok(found)
}

fn set_theme(text: &str, value: &str) -> Result<Vec<u8>> {
    let mut out = text.to_owned();
    if let Some(binding) = binding(text)? {
        if binding.value != value {
            out.replace_range(binding.range, &format!("\"{value}\""));
        }
    } else {
        let nl = if text.contains("\r\n") { "\r\n" } else { "\n" };
        if !out.is_empty() && !out.ends_with('\n') {
            out.push_str(nl);
        }
        out.push_str(&format!("color_theme = \"{value}\"{nl}"));
    }
    if out.len() as u64 > MAX_TOOL_CONFIG_BYTES {
        return Err(invalid("generated configuration exceeds 8 MiB"));
    }
    Ok(out.into_bytes())
}

pub(crate) fn is_owned_theme(bytes: &[u8]) -> bool {
    bytes.starts_with(palette::HEADER.as_bytes())
}

/// Disk evidence only. Named themes and process overrides are not resolved.
pub(crate) fn references_owned_theme(env: &SlateEnv, text: &str) -> Result<bool> {
    let owned = literal_path(env)?;
    Ok(binding(text)?.is_some_and(|binding| binding.value == owned))
}

pub(crate) fn matches_generated_theme(bytes: &[u8], theme: &ThemeVariant) -> Result<bool> {
    Ok(bytes == palette::render(theme)?.as_bytes())
}

/// Clean disconnects only our exact reference. Previous personal themes live
/// in pre-apply snapshots; clean is not restoration of that historical choice.
pub(crate) fn clean_config(env: &SlateEnv, bytes: &[u8]) -> Result<Vec<u8>> {
    let text = std::str::from_utf8(bytes).map_err(|_| invalid("configuration is not UTF-8"))?;
    let owned = literal_path(env)?;
    if binding(text)?.is_some_and(|b| b.value == owned) {
        set_theme(text, "Default")
    } else {
        Ok(bytes.to_vec())
    }
}

struct FilePlan {
    path: PathBuf,
    destination: PathBuf,
    original: Option<Source>,
    bytes: Vec<u8>,
}

impl FilePlan {
    fn read(env: &SlateEnv, path: PathBuf) -> Result<Self> {
        recovery_paths::validate_file_path(env, &path, "btop")?;
        let destination = file_read::directory_alias_target(&path)
            .ok_or_else(|| invalid("cannot resolve configuration destination"))?;
        let original = file_read::read(&path, MAX_TOOL_CONFIG_BYTES, Links::Reject)
            .map_err(|e| invalid(&e.to_string()))?;
        Ok(Self {
            path,
            destination,
            original,
            bytes: Vec::new(),
        })
    }

    fn verify(&self, env: &SlateEnv) -> Result<()> {
        let current = Self::read(env, self.path.clone())?;
        if self.original != current.original || self.destination != current.destination {
            return Err(invalid(
                "configuration changed while preparing; retry without overwriting the edit",
            ));
        }
        Ok(())
    }

    fn publish(self, env: &SlateEnv) -> Result<()> {
        self.verify(env)?;
        if self
            .original
            .as_ref()
            .is_some_and(|s| s.bytes == self.bytes)
        {
            return Ok(());
        }
        std::fs::create_dir_all(self.path.parent().expect("absolute file parent"))?;
        self.verify(env)?;
        atomic_write_synced(&self.path, &self.bytes)
    }
}

pub(super) fn apply(env: &SlateEnv, theme: &ThemeVariant) -> Result<()> {
    let rendered = palette::render(theme)?;
    let value = literal_path(env)?;
    // Shared checkpoints own recovery for coordinated applies. Validate the
    // same targets even for direct adapter calls before any writes.
    recovery_paths::targets(
        env,
        [BtopAdapter::config_path(env), BtopAdapter::theme_path(env)],
        "btop",
    )?;
    let mut config = FilePlan::read(env, BtopAdapter::config_path(env))?;
    let mut asset = FilePlan::read(env, BtopAdapter::theme_path(env))?;
    if asset
        .original
        .as_ref()
        .is_some_and(|s| !is_owned_theme(&s.bytes))
    {
        return Err(invalid("slate-sync.theme already exists without Slate ownership; preserve or rename it before connecting"));
    }
    let text = config.original.as_ref().map_or(Ok(""), |s| {
        std::str::from_utf8(&s.bytes).map_err(|_| invalid("configuration is not UTF-8"))
    })?;
    config.bytes = set_theme(text, &value)?;
    asset.bytes = rendered.into_bytes();
    config.verify(env)?;
    asset.publish(env)?;
    config.publish(env)
}

#[cfg(test)]
mod tests;
