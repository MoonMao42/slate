//! Explicit pairing edits save only auto.toml. Watchers reread it on appearance
//! events; applying a theme and changing watcher enablement are separate actions.
use super::{
    auto_theme::read_auto_theme_value,
    file_read::{self, Links, Source, MAX_DOCUMENT_BYTES},
    flags::{parse_document, set_value},
    recovery_paths, AutoConfig, ConfigWriteGuard,
};
use crate::{
    env::SlateEnv,
    error::{Result, SlateError},
    theme::{ThemeAppearance, ThemeRegistry},
};
use std::{
    fs,
    path::{Path, PathBuf},
};
use toml_edit::DocumentMut;

fn failure(reason: impl std::fmt::Display) -> SlateError {
    SlateError::InvalidConfig(format!(
        "Auto-theme pairing: {reason}; configuration contents omitted"
    ))
}

/// Validate exact embedded IDs without consulting a profile or native programs.
pub(crate) fn validate(dark: Option<&str>, light: Option<&str>) -> Result<()> {
    let registry = ThemeRegistry::new()?;
    for (value, appearance, option) in [
        (dark, ThemeAppearance::Dark, "--dark"),
        (light, ThemeAppearance::Light, "--light"),
    ] {
        if let Some(id) = value {
            let theme = registry.get(id).ok_or_else(|| failure(format!("{option} requires an exact known theme ID; use `slate list --ids` to inspect choices")))?;
            if theme.appearance != appearance {
                return Err(failure(format!(
                    "{option} requires a theme of the matching appearance"
                )));
            }
        }
    }
    Ok(())
}

fn read(env: &SlateEnv, path: &Path) -> Result<Option<Source>> {
    recovery_paths::validate_file_path(env, path, "auto-theme pairing")?;
    file_read::read(path, MAX_DOCUMENT_BYTES, Links::Reject).map_err(failure)
}

fn document(path: &Path, source: &Option<Source>) -> Result<DocumentMut> {
    match source {
        Some(source) => parse_document(
            path,
            std::str::from_utf8(&source.bytes).map_err(|_| failure("invalid UTF-8"))?,
        ),
        None => Ok(DocumentMut::default()),
    }
}

fn values(doc: &DocumentMut) -> Result<AutoConfig> {
    Ok(AutoConfig {
        dark_theme: read_auto_theme_value(doc, "dark_theme")?,
        light_theme: read_auto_theme_value(doc, "light_theme")?,
    })
}

#[derive(Clone, Copy)]
pub(crate) enum SlotEdit<'a> {
    Keep,
    Set(&'a str),
    Clear,
}

impl<'a> SlotEdit<'a> {
    fn value(self) -> Option<&'a str> {
        match self {
            Self::Set(value) => Some(value),
            Self::Keep | Self::Clear => None,
        }
    }
}

/// Preserve comment text attached to a removed assignment as standalone footer
/// comments. Other values keep their original decoration and spelling. Do not
/// retain the removed value itself or turn a comment into executable TOML.
fn clear_slot(doc: &mut DocumentMut, name: &str) {
    let Some((key, item)) = doc.remove_entry(name) else {
        return;
    };
    let mut trailing = doc.trailing().as_str().unwrap_or_default().to_owned();
    for raw in [
        key.leaf_decor().prefix(),
        item.as_value().and_then(|value| value.decor().suffix()),
    ]
    .into_iter()
    .flatten()
    {
        let Some(text) = raw.as_str().filter(|text| text.contains('#')) else {
            continue;
        };
        if !trailing.ends_with('\n') {
            trailing.push('\n');
        }
        trailing.push_str(text);
        if !trailing.ends_with('\n') {
            trailing.push('\n');
        }
    }
    doc.set_trailing(trailing);
}

pub(crate) fn inspect(env: &SlateEnv) -> Result<AutoConfig> {
    let path = env.managed_file("auto.toml");
    values(&document(&path, &read(env, &path)?)?)
}

pub(crate) struct PreparedPairing {
    env: SlateEnv,
    path: PathBuf,
    location: PathBuf,
    original: Option<Source>,
    desired: Option<Vec<u8>>,
    pub(crate) before: AutoConfig,
    pub(crate) after: AutoConfig,
}

impl PreparedPairing {
    #[cfg(test)]
    pub(crate) fn capture(env: &SlateEnv, dark: Option<&str>, light: Option<&str>) -> Result<Self> {
        Self::capture_edits(
            env,
            dark.map_or(SlotEdit::Keep, SlotEdit::Set),
            light.map_or(SlotEdit::Keep, SlotEdit::Set),
        )
    }

    pub(crate) fn capture_edits(
        env: &SlateEnv,
        dark: SlotEdit<'_>,
        light: SlotEdit<'_>,
    ) -> Result<Self> {
        validate(dark.value(), light.value())?;
        if matches!((dark, light), (SlotEdit::Keep, SlotEdit::Keep)) {
            return Err(failure("select a slot to set or clear before saving"));
        }
        let path = env.managed_file("auto.toml");
        let original = read(env, &path)?;
        let location = file_read::directory_alias_target(&path)
            .ok_or_else(|| failure("cannot resolve pairing parent"))?;
        let mut doc = document(&path, &original)?;
        let before = values(&doc)?;
        for (key, edit) in [("dark_theme", dark), ("light_theme", light)] {
            match edit {
                SlotEdit::Set(value)
                    if doc.get(key).and_then(|item| item.as_str()) != Some(value) =>
                {
                    set_value(doc.as_table_mut(), key, toml_edit::Value::from(value));
                }
                SlotEdit::Clear => clear_slot(&mut doc, key),
                SlotEdit::Keep | SlotEdit::Set(_) => {}
            }
        }
        let after = values(&doc)?;
        let desired = doc.to_string();
        if desired.len() as u64 > MAX_DOCUMENT_BYTES {
            return Err(failure("updated document exceeds 256 KiB"));
        }
        // Recheck emitted TOML, including relocated raw comment decoration,
        // before allocating a checkpoint or publishing anything.
        parse_document(&path, &desired)?;
        let desired = desired.into_bytes();
        // Clearing an unset slot must not create a new empty preference file.
        // Existing files, including newly empty files, are deliberately retained.
        let desired = if original.is_none() && desired.is_empty() {
            None
        } else {
            Some(desired)
        };
        let plan = Self {
            env: env.clone(),
            path,
            location,
            original,
            desired,
            before,
            after,
        };
        plan.verify()?;
        if plan.changed() {
            recovery_paths::targets(env, [plan.path.clone()], "Pairing")?;
        }
        Ok(plan)
    }

    pub(crate) fn changed(&self) -> bool {
        self.original.as_ref().map(|source| source.bytes.as_slice()) != self.desired.as_deref()
    }

    fn verify(&self) -> Result<()> {
        if read(&self.env, &self.path)? != self.original
            || file_read::directory_alias_target(&self.path).as_ref() != Some(&self.location)
        {
            return Err(failure(
                "file or parent changed after preparation; retry without overwriting later edits",
            ));
        }
        Ok(())
    }

    pub(crate) fn save(&self) -> Result<Option<String>> {
        self.save_with(|_| {})
    }

    fn save_with(&self, after_checkpoint: impl FnOnce(&str)) -> Result<Option<String>> {
        let _guard = ConfigWriteGuard::acquire(&self.env)?;
        self.verify()?;
        if !self.changed() {
            return Ok(None);
        }
        let targets = recovery_paths::targets(&self.env, [self.path.clone()], "Pairing")?;
        let point = super::snapshot_config_targets_with_env(&self.env, &targets)?;
        after_checkpoint(&point.id);
        let publish = || {
            self.verify()?;
            fs::create_dir_all(
                self.path
                    .parent()
                    .ok_or_else(|| failure("missing parent"))?,
            )?;
            self.verify()?;
            super::state_files::atomic_write_synced_mode(
                &self.path,
                self.desired
                    .as_deref()
                    .ok_or_else(|| failure("unexpected absent pairing update"))?,
                self.original
                    .as_ref()
                    .and_then(|source| source.mode)
                    .or(Some(0o600)),
            )
        };
        publish().map_err(|error| failure(format!("pairing was not confirmed saved: {error}. Inspect file recovery with `slate restore {} --dry-run`", point.id)))?;
        Ok(Some(point.id))
    }
}

#[cfg(test)]
mod tests;
