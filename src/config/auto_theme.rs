use super::flags::{read_document, set_value, write_document};
use super::AutoConfig;
use crate::error::{Result, SlateError};
use std::path::Path;
use toml_edit::DocumentMut;

pub(super) fn read_auto_theme_value(doc: &DocumentMut, key: &str) -> Result<Option<String>> {
    match doc.get(key) {
        Some(item) => item
            .as_str()
            .map(|value| value.to_string())
            .map(Some)
            .ok_or_else(|| {
                SlateError::InvalidConfig(format!("auto.toml field '{}' must be a string", key))
            }),
        None => Ok(None),
    }
}

pub(super) fn read_auto_config(base_path: &Path) -> Result<Option<AutoConfig>> {
    let path = base_path.join("auto.toml");

    let Some(doc) = read_document(&path)? else {
        return Ok(None);
    };
    let dark_theme = read_auto_theme_value(&doc, "dark_theme")?;
    let light_theme = read_auto_theme_value(&doc, "light_theme")?;

    Ok(Some(AutoConfig {
        dark_theme,
        light_theme,
    }))
}

pub(super) fn write_auto_config(
    base_path: &Path,
    dark_theme: Option<&str>,
    light_theme: Option<&str>,
) -> Result<()> {
    let path = base_path.join("auto.toml");
    let mut doc = read_document(&path)?.unwrap_or_default();
    // Validate both saved fields from this one read, but only rewrite requested
    // values. Unspecified pairing, comments and unrelated keys stay intact.
    read_auto_theme_value(&doc, "dark_theme")?;
    read_auto_theme_value(&doc, "light_theme")?;
    for (key, value) in [("dark_theme", dark_theme), ("light_theme", light_theme)] {
        if let Some(value) = value {
            set_value(doc.as_table_mut(), key, toml_edit::Value::from(value));
        }
    }

    write_document(&path, &doc)
}
