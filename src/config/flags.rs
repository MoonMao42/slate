use crate::error::Result;
use std::fs;
use std::path::Path;
use toml_edit::DocumentMut;

pub(super) fn parse_toml_document(content: &str) -> Result<DocumentMut> {
    if content.trim().is_empty() {
        Ok(DocumentMut::new())
    } else {
        Ok(content.parse::<DocumentMut>()?)
    }
}

pub(super) fn config_flag(base_path: &Path, section: &str, key: &str) -> Result<Option<bool>> {
    let config_path = base_path.join("config.toml");
    if !config_path.exists() {
        return Ok(None);
    }

    let content = fs::read_to_string(&config_path)?;
    let doc = parse_toml_document(&content)?;
    Ok(doc
        .get(section)
        .and_then(|table| table.get(key))
        .and_then(|value| value.as_bool()))
}

pub(super) fn set_config_flag(
    base_path: &Path,
    section: &str,
    key: &str,
    enabled: bool,
) -> Result<()> {
    let config_path = base_path.join("config.toml");
    let mut doc = if config_path.exists() {
        parse_toml_document(&fs::read_to_string(&config_path)?)?
    } else {
        DocumentMut::new()
    };

    if !doc.contains_key(section) {
        doc.insert(section, toml_edit::table());
    }

    doc[section][key] = toml_edit::value(enabled);
    write_document(&config_path, &doc)
}

pub(super) fn write_document(path: &Path, document: &DocumentMut) -> Result<()> {
    super::state_files::atomic_write_synced(path, document.to_string().as_bytes())
}
