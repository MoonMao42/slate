use super::flags::{parse_toml_document, write_document};
use super::AutoConfig;
use crate::error::{Result, SlateError};
use std::fs;
use std::path::Path;
use toml_edit::DocumentMut;

fn read_auto_theme_value(doc: &DocumentMut, key: &str) -> Result<Option<String>> {
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

    if !path.exists() {
        return Ok(None);
    }

    let content = fs::read_to_string(&path)?;
    let doc = parse_toml_document(&content)?;
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
    let current = read_auto_config(base_path)?;
    let path = base_path.join("auto.toml");
    let mut doc = if path.exists() {
        parse_toml_document(&fs::read_to_string(&path)?)?
    } else {
        DocumentMut::new()
    };

    let final_dark = dark_theme
        .map(String::from)
        .or(current.as_ref().and_then(|c| c.dark_theme.clone()));
    let final_light = light_theme
        .map(String::from)
        .or(current.as_ref().and_then(|c| c.light_theme.clone()));

    if let Some(dark) = final_dark {
        doc["dark_theme"] = toml_edit::value(dark);
    } else {
        doc.remove("dark_theme");
    }
    if let Some(light) = final_light {
        doc["light_theme"] = toml_edit::value(light);
    } else {
        doc.remove("light_theme");
    }

    write_document(&path, &doc)
}
