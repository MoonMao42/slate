use super::file_read::{read_text_with_links, Links, MAX_DOCUMENT_BYTES};
use crate::env::SlateEnv;
use crate::error::{Result, SlateError};
use std::path::Path;
use toml_edit::DocumentMut;

pub(super) fn read_document(path: &Path) -> Result<Option<DocumentMut>> {
    read_document_with_links(path, Links::Follow)
}

fn read_document_with_links(path: &Path, links: Links) -> Result<Option<DocumentMut>> {
    read_text_with_links(path, MAX_DOCUMENT_BYTES, links)?
        .map(|content| parse_document(path, &content))
        .transpose()
}

pub(super) fn parse_document(path: &Path, content: &str) -> Result<DocumentMut> {
    content.parse::<DocumentMut>().map_err(|err| {
        // TomlError's Display includes source lines. Keep only location.
        let location = err
            .span()
            .map(|span| {
                let prefix = &content.as_bytes()[..span.start.min(content.len())];
                let line = prefix.iter().filter(|&&b| b == b'\n').count() + 1;
                format!(" at line {line}")
            })
            .unwrap_or_default();
        SlateError::ConfigParseError(
            path.display().to_string(),
            format!("invalid TOML{location}"),
        )
    })
}

pub(super) fn config_flag(base_path: &Path, section: &str, key: &str) -> Result<Option<bool>> {
    config_flag_with_links(base_path, section, key, Links::Follow)
}

/// Read-only diagnostics use a stricter path contract than ordinary linked
/// dotfile reads, but share the parser, field validation and absence semantics.
pub(super) fn inspect_config_flag(
    env: &SlateEnv,
    section: &str,
    key: &str,
) -> Result<Option<bool>> {
    super::recovery_paths::validate_file_path(
        env,
        &env.managed_file("config.toml"),
        "configuration preference",
    )?;
    config_flag_with_links(env.config_dir(), section, key, Links::Reject)
}

fn config_flag_with_links(
    base_path: &Path,
    section: &str,
    key: &str,
    links: Links,
) -> Result<Option<bool>> {
    let config_path = base_path.join("config.toml");
    let Some(doc) = read_document_with_links(&config_path, links)? else {
        return Ok(None);
    };
    let Some(item) = doc.get(section) else {
        return Ok(None);
    };
    let table = item.as_table_like().ok_or_else(|| {
        SlateError::InvalidConfig(format!("config.toml section [{section}] must be a table"))
    })?;
    table
        .get(key)
        .map(|value| {
            value.as_bool().ok_or_else(|| {
                SlateError::InvalidConfig(format!(
                    "config.toml [{section}].{key} must be a boolean"
                ))
            })
        })
        .transpose()
}

pub(super) fn set_config_flag(
    base_path: &Path,
    section: &str,
    key: &str,
    enabled: bool,
) -> Result<()> {
    let config_path = base_path.join("config.toml");
    let mut doc = read_document(&config_path)?.unwrap_or_default();
    edit_flag(&mut doc, section, key, enabled)?;
    write_document(&config_path, &doc)
}

fn edit_flag(doc: &mut DocumentMut, section: &str, key: &str, enabled: bool) -> Result<()> {
    if !doc.contains_key(section) {
        doc.insert(section, toml_edit::table());
    }

    let table = doc
        .get_mut(section)
        .and_then(|item| item.as_table_like_mut())
        .ok_or_else(|| {
            SlateError::InvalidConfig(format!("config.toml section [{section}] must be a table"))
        })?;
    set_value(table, key, toml_edit::Value::from(enabled));
    Ok(())
}

/// Render from the exact captured document; do not reread or write the path.
pub(super) fn flag_bytes(
    path: &Path,
    original: Option<&[u8]>,
    section: &str,
    key: &str,
    enabled: bool,
) -> Result<Vec<u8>> {
    let mut doc = match original {
        Some(bytes) => parse_document(
            path,
            std::str::from_utf8(bytes).map_err(|_| {
                SlateError::ConfigParseError(path.display().to_string(), "invalid UTF-8".into())
            })?,
        )?,
        None => DocumentMut::default(),
    };
    edit_flag(&mut doc, section, key, enabled)?;
    Ok(doc.to_string().into_bytes())
}

pub(super) fn set_value(
    table: &mut dyn toml_edit::TableLike,
    key: &str,
    mut value: toml_edit::Value,
) {
    if let Some(item) = table.get_mut(key) {
        if let Some(previous) = item.as_value() {
            *value.decor_mut() = previous.decor().clone();
        }
        // Replacing via insert also replaces key decoration (including comments
        // above the first key). Mutate the existing item to retain that text.
        *item = toml_edit::Item::Value(value);
    } else {
        table.insert(key, toml_edit::Item::Value(value));
    }
}

pub(super) fn write_document(path: &Path, document: &DocumentMut) -> Result<()> {
    let content = document.to_string();
    if content.len() as u64 > MAX_DOCUMENT_BYTES {
        return Err(SlateError::ConfigWriteError(
            path.display().to_string(),
            "document exceeds 256 KiB limit".into(),
        ));
    }
    super::state_files::atomic_write_synced(path, content.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn config_flag_distinguishes_absence_from_invalid_types_without_writing() {
        let td = tempfile::TempDir::new().unwrap();
        let path = td.path().join("config.toml");
        assert_eq!(
            config_flag(td.path(), "auto_theme", "enabled").unwrap(),
            None
        );
        assert_eq!(fs::read_dir(td.path()).unwrap().count(), 0);
        for (text, expected) in [
            ("[auto_theme]\nenabled = true\n", Some(true)),
            ("auto_theme = { enabled = false }\n", Some(false)),
            ("[auto_theme]\n", None),
        ] {
            fs::write(&path, text).unwrap();
            assert_eq!(
                config_flag(td.path(), "auto_theme", "enabled").unwrap(),
                expected
            );
            assert_eq!(fs::read_to_string(&path).unwrap(), text);
        }
        for text in [
            "auto_theme = false\n",
            "[auto_theme]\nenabled = 'PRIVATE_BAD_VALUE'\n",
        ] {
            fs::write(&path, text).unwrap();
            let err = config_flag(td.path(), "auto_theme", "enabled").unwrap_err();
            assert!(!err.to_string().contains("PRIVATE_BAD_VALUE"));
            assert_eq!(fs::read_to_string(&path).unwrap(), text);
        }
    }
}
