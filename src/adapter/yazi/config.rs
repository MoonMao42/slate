//! Three bounded files, assets before selection; shared coordinator owns recovery.
use super::{palette, YaziAdapter};
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
use std::path::PathBuf;
use toml_edit::{DocumentMut, Item, TableLike, Value};

const FLAVOR: &str = "slate-sync";

fn invalid(reason: &str) -> SlateError {
    SlateError::InvalidConfig(format!("Yazi: {reason}; file contents omitted"))
}

fn parse(bytes: &[u8]) -> Result<DocumentMut> {
    if bytes.len() as u64 > MAX_TOOL_CONFIG_BYTES {
        return Err(invalid("theme.toml exceeds 8 MiB"));
    }
    std::str::from_utf8(bytes)
        .map_err(|_| invalid("theme.toml is not UTF-8"))?
        .parse()
        .map_err(|_| invalid("theme.toml contains invalid TOML"))
}

fn flavor(doc: &mut DocumentMut) -> Result<Option<&mut dyn TableLike>> {
    doc.get_mut("flavor")
        .map(|section| {
            section
                .as_table_like_mut()
                .ok_or_else(|| invalid("[flavor] must be a table"))
        })
        .transpose()
}

fn set_flavor(bytes: &[u8]) -> Result<Vec<u8>> {
    let mut doc = parse(bytes)?;
    if !doc.contains_key("flavor") {
        doc["flavor"] = toml_edit::table();
    }
    let section = flavor(&mut doc)?.expect("created table");
    for key in ["dark", "light"] {
        if section.get(key).is_some_and(|item| item.as_str().is_none()) {
            return Err(invalid("flavor selections must be strings"));
        }
    }
    let mut changed = false;
    for key in ["dark", "light"] {
        if section.get(key).and_then(Item::as_str) != Some(FLAVOR) {
            let mut value = Value::from(FLAVOR);
            if let Some(old) = section.get(key).and_then(Item::as_value) {
                *value.decor_mut() = old.decor().clone();
            }
            section.insert(key, Item::Value(value));
            changed = true;
        }
    }
    if !changed {
        return Ok(bytes.to_vec());
    }
    Ok(doc.to_string().into_bytes())
}

pub(crate) fn clean_config(bytes: &[u8]) -> Result<Vec<u8>> {
    let mut doc = parse(bytes)?;
    let Some(section) = flavor(&mut doc)? else {
        return Ok(bytes.to_vec());
    };
    let mut changed = false;
    for key in ["dark", "light"] {
        if section.get(key).and_then(Item::as_str) == Some(FLAVOR) {
            section.remove(key);
            changed = true;
        }
    }
    Ok(if changed {
        doc.to_string().into_bytes()
    } else {
        bytes.to_vec()
    })
}

pub(crate) fn owns_flavor(bytes: &[u8]) -> bool {
    bytes.starts_with(palette::HEADER.as_bytes())
}
pub(crate) fn owns_syntax(bytes: &[u8]) -> bool {
    bytes.starts_with(palette::XML_HEADER.as_bytes())
}

pub(crate) fn generated_assets(theme: &ThemeVariant) -> Result<(String, String)> {
    palette::render(theme)
}

/// Observations share the writer's TOML/slot rules, without editing the file.
pub(crate) fn inspect_selection(bytes: &[u8]) -> Result<([bool; 2], bool)> {
    let mut doc = parse(bytes)?;
    let personal_sections = doc.iter().any(|(key, _)| key != "flavor");
    let mut selected = [false; 2];
    if let Some(section) = flavor(&mut doc)? {
        for (index, key) in ["dark", "light"].into_iter().enumerate() {
            if let Some(item) = section.get(key) {
                let value = item
                    .as_str()
                    .ok_or_else(|| invalid("flavor selections must be strings"))?;
                selected[index] = value == FLAVOR;
            }
        }
    }
    Ok((selected, personal_sections))
}

struct File {
    path: PathBuf,
    destination: PathBuf,
    source: Option<Source>,
    bytes: Vec<u8>,
}
impl File {
    fn read(env: &SlateEnv, path: PathBuf) -> Result<Self> {
        recovery_paths::validate_file_path(env, &path, "Yazi")?;
        let destination = file_read::directory_alias_target(&path)
            .ok_or_else(|| invalid("cannot resolve file destination"))?;
        let source = file_read::read(&path, MAX_TOOL_CONFIG_BYTES, Links::Reject)
            .map_err(|_| invalid("cannot safely read a regular file up to 8 MiB"))?;
        Ok(Self {
            path,
            destination,
            source,
            bytes: Vec::new(),
        })
    }
    fn verify(&self, env: &SlateEnv) -> Result<()> {
        let current = Self::read(env, self.path.clone())?;
        if current.source != self.source || current.destination != self.destination {
            return Err(invalid(
                "files changed while preparing; retry without overwriting the external edit",
            ));
        }
        Ok(())
    }
    fn publish(&self, env: &SlateEnv) -> Result<()> {
        self.verify(env)?;
        if self.source.as_ref().is_some_and(|s| s.bytes == self.bytes) {
            return Ok(());
        }
        std::fs::create_dir_all(self.path.parent().expect("absolute parent"))?;
        self.verify(env)?;
        atomic_write_synced(&self.path, &self.bytes)
    }
}

pub(super) fn apply(env: &SlateEnv, theme: &ThemeVariant) -> Result<()> {
    let (flavor, syntax) = palette::render(theme)?;
    let paths = YaziAdapter::paths(env);
    if recovery_paths::targets(env, paths.clone(), "Yazi")?.len() != 3 {
        return Err(invalid(
            "the three configuration files must have distinct destinations",
        ));
    }
    let [config_path, flavor_path, syntax_path] = paths;
    let mut config = File::read(env, config_path)?;
    let mut asset = File::read(env, flavor_path)?;
    let mut xml = File::read(env, syntax_path)?;
    for (file, owns) in [
        (&asset, owns_flavor as fn(&[u8]) -> bool),
        (&xml, owns_syntax),
    ] {
        if file.source.as_ref().is_some_and(|s| !owns(&s.bytes)) {
            return Err(invalid("a same-named flavor asset lacks Slate ownership; preserve or rename it before connecting"));
        }
    }
    config.bytes = set_flavor(config.source.as_ref().map_or(&[], |s| s.bytes.as_slice()))?;
    asset.bytes = flavor.into_bytes();
    xml.bytes = syntax.into_bytes();
    for file in [&config, &asset, &xml] {
        if file.bytes.len() as u64 > MAX_TOOL_CONFIG_BYTES {
            return Err(invalid("generated file exceeds 8 MiB"));
        }
        file.verify(env)?;
    }
    // A failure after an asset publish is partial; the caller retains its exact
    // three-file checkpoint. Never claim an all-or-nothing filesystem transaction.
    asset.publish(env)?;
    config.verify(env)?;
    xml.publish(env)?;
    config.publish(env)
}

#[cfg(test)]
mod tests;
