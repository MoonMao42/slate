//! Shared, side-effect-free transformations for clean execution and inspection.
use crate::config::{atomic_write_synced, read_snapshot_source, MAX_SNAPSHOT_BYTES};
use crate::env::SlateEnv;
use crate::error::Result;
use std::path::{Component, Path};

mod alacritty;

#[cfg(test)]
mod tests;

pub(super) enum Edit {
    Keep(Option<&'static str>),
    Replace(Vec<u8>),
    Remove,
}

impl Edit {
    pub(super) fn replaced(before: &[u8], after: Vec<u8>) -> Self {
        if before == after {
            Self::Keep(None)
        } else {
            Self::Replace(after)
        }
    }
}

pub(super) fn yazi(bytes: &[u8]) -> Result<Edit> {
    Ok(Edit::replaced(
        bytes,
        crate::adapter::yazi::config::clean_config(bytes)?,
    ))
}

pub(super) fn zellij(bytes: &[u8]) -> Result<Edit> {
    Ok(Edit::replaced(
        bytes,
        crate::adapter::zellij::config::clean_config(bytes)?,
    ))
}

pub(super) fn zellij_theme(bytes: &[u8]) -> Result<Edit> {
    Ok(if crate::adapter::zellij::config::owns_theme(bytes) {
        Edit::Remove
    } else {
        Edit::Keep(Some("Zellij theme lacks Slate ownership; preserved"))
    })
}

pub(super) fn yazi_asset(bytes: &[u8], syntax: bool) -> Result<Edit> {
    let owned = if syntax {
        crate::adapter::yazi::config::owns_syntax(bytes)
    } else {
        crate::adapter::yazi::config::owns_flavor(bytes)
    };
    Ok(if owned {
        Edit::Remove
    } else {
        Edit::Keep(Some("Yazi asset lacks Slate ownership; preserved"))
    })
}

pub(super) fn apply(path: &Path, edit: impl FnOnce(&[u8]) -> Result<Edit>) -> Result<()> {
    let mut remaining = MAX_SNAPSHOT_BYTES;
    let Some(source) = read_snapshot_source(path, &mut remaining)? else {
        return Ok(());
    };
    match edit(&source.bytes)? {
        Edit::Keep(note) => {
            if let Some(note) = note {
                eprintln!("warning: {} left unchanged: {note}", path.display());
            }
            Ok(())
        }
        Edit::Replace(bytes) => atomic_write_synced(path, &bytes),
        Edit::Remove => Ok(std::fs::remove_file(path)?),
    }
}

pub(super) fn starship(bytes: &[u8]) -> Result<Edit> {
    let Ok(content) = std::str::from_utf8(bytes) else {
        return Ok(Edit::Keep(Some(
            "Starship is not UTF-8; inspect its Slate palette manually",
        )));
    };
    let Ok(mut doc) = content.parse::<toml_edit::DocumentMut>() else {
        return Ok(Edit::Keep(Some(
            "Invalid Starship TOML; inspect its Slate palette manually",
        )));
    };
    let mut changed = false;
    if doc.get("palette").and_then(|value| value.as_str()) == Some("slate") {
        doc.remove("palette");
        changed = true;
    }
    if let Some(palettes) = doc
        .get_mut("palettes")
        .and_then(|value| value.as_table_mut())
    {
        if palettes.remove("slate").is_some() {
            changed = true;
        }
        if palettes.is_empty() {
            doc.remove("palettes");
        }
    }
    Ok(if changed {
        Edit::replaced(bytes, doc.to_string().into_bytes())
    } else {
        Edit::Keep(None)
    })
}

pub(super) fn btop(env: &SlateEnv, bytes: &[u8]) -> Result<Edit> {
    Ok(Edit::replaced(
        bytes,
        crate::adapter::btop::config::clean_config(env, bytes)?,
    ))
}

pub(super) fn btop_theme(bytes: &[u8]) -> Result<Edit> {
    Ok(if crate::adapter::btop::config::is_owned_theme(bytes) {
        Edit::Remove
    } else {
        Edit::Keep(Some("btop theme lacks Slate ownership; preserved"))
    })
}

pub(super) fn kitty(env: &SlateEnv, bytes: &[u8]) -> Result<Edit> {
    let root = env.config_dir().join("managed/kitty");
    let mut cleaned = Vec::with_capacity(bytes.len());
    for line in crate::adapter::kitty_config::lines(bytes) {
        if line.value(b"include").is_some_and(|value| {
            std::str::from_utf8(value).is_ok_and(|path| is_managed_path(path, &root))
        }) {
            continue;
        }
        if line.value(b"listen_on").is_some_and(|value| {
            value
                .strip_prefix(b"unix:")
                .and_then(|value| std::str::from_utf8(value).ok())
                .is_some_and(|value| {
                    literal_absolute_path(value)
                        && Path::new(value).file_name() == Some("kitty-slate".as_ref())
                })
        }) {
            continue;
        }
        cleaned.extend_from_slice(line.raw);
    }
    Ok(Edit::replaced(bytes, cleaned))
}

// Only literal, absolute descendants of the owned tree are cleanup targets.
// Do not resolve symlinks, expand variables, or guess at parent traversals.
fn literal_absolute_path(value: &str) -> bool {
    let path = Path::new(value);
    path.is_absolute()
        && !value.contains('$')
        && !value.chars().any(char::is_control)
        && !path.components().any(|part| part == Component::ParentDir)
}

fn is_managed_path(value: &str, root: &Path) -> bool {
    literal_absolute_path(value) && Path::new(value) != root && Path::new(value).starts_with(root)
}

pub(super) fn alacritty(env: &SlateEnv, bytes: &[u8]) -> Result<Edit> {
    alacritty::clean(env, bytes)
}

pub(super) fn opencode(bytes: &[u8], path: &Path) -> Result<Edit> {
    let Ok(content) = std::str::from_utf8(bytes) else {
        return Ok(Edit::Keep(Some(
            "OpenCode is not UTF-8; inspect its theme manually",
        )));
    };
    let Ok(config) = crate::adapter::opencode::config::Document::parse(content, path) else {
        return Ok(Edit::Keep(Some(
            "Invalid or ambiguous OpenCode JSON/JSONC; inspect its theme manually",
        )));
    };
    match config.without_system_theme(path)? {
        Some(output) => Ok(Edit::replaced(bytes, output)),
        None => Ok(Edit::Remove),
    }
}
