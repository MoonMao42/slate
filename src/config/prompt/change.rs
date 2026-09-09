use super::*;
use crate::config::{
    file_read::{self, Links, Source, MAX_DOCUMENT_BYTES, MAX_STATE_BYTES, MAX_TOOL_CONFIG_BYTES},
    recovery_paths,
    state_files::atomic_write_synced_mode,
    ConfigWriteGuard,
};
use crate::theme::ThemeRegistry;
use std::path::PathBuf;

struct File {
    path: PathBuf,
    destination: PathBuf,
    original: Option<Source>,
    desired: Vec<u8>,
    limit: u64,
}

impl File {
    fn read(env: &SlateEnv, path: PathBuf, limit: u64) -> Result<Self> {
        Self::read_with(env, path, limit, |path| {
            file_read::read(path, limit, Links::Reject)
                .map_err(|_| invalid("cannot safely read a configuration file"))
        })
    }

    fn read_with(
        env: &SlateEnv,
        path: PathBuf,
        limit: u64,
        read: impl FnOnce(&std::path::Path) -> Result<Option<Source>>,
    ) -> Result<Self> {
        recovery_paths::validate_file_path(env, &path, "prompt style")?;
        let destination = file_read::directory_alias_target(&path)
            .ok_or_else(|| invalid("cannot resolve a configuration destination"))?;
        let original = read(&path)?;
        // Pin before reading: identical bytes/inodes (or two absent files) do
        // not imply the same destination after a parent alias is retargeted.
        if file_read::directory_alias_target(&path).as_ref() != Some(&destination) {
            return Err(invalid(
                "configuration destination changed while reading; review again",
            ));
        }
        Ok(Self {
            path,
            destination,
            original,
            desired: Vec::new(),
            limit,
        })
    }
    fn text(&self) -> Result<&str> {
        self.original
            .as_ref()
            .map(|s| {
                std::str::from_utf8(&s.bytes).map_err(|_| invalid("configuration is not UTF-8"))
            })
            .unwrap_or(Ok(""))
    }
    fn changed(&self) -> bool {
        self.original.as_ref().map(|s| s.bytes.as_slice()) != Some(self.desired.as_slice())
    }
    fn verify(&self, env: &SlateEnv) -> Result<()> {
        let now = Self::read(env, self.path.clone(), self.limit)?;
        if now.original != self.original || now.destination != self.destination {
            return Err(invalid(
                "configuration or destination changed after review; review again",
            ));
        }
        Ok(())
    }
    fn publish(&self, env: &SlateEnv) -> Result<()> {
        self.verify(env)?;
        if !self.changed() {
            return Ok(());
        }
        std::fs::create_dir_all(
            self.path
                .parent()
                .ok_or_else(|| invalid("missing parent"))?,
        )?;
        self.verify(env)?;
        atomic_write_synced_mode(
            &self.path,
            &self.desired,
            self.original.as_ref().and_then(|s| s.mode).or(Some(0o600)),
        )
    }
}

#[derive(Serialize)]
pub(crate) struct PromptPreview {
    schema_version: u8,
    pub style: PromptStyle,
    pub theme: String,
    pub example: &'static str,
    pub starship_config_override: Option<CapturedOverride>,
    pub changes: Vec<Change>,
    pub notes: Vec<&'static str>,
}

#[derive(Serialize)]
pub(crate) struct CapturedOverride {
    pub path: String,
    pub path_is_lossy: bool,
    scope: &'static str,
}

#[derive(Serialize)]
pub(crate) struct Change {
    pub path: PathBuf,
    pub changed: bool,
}

pub(crate) struct PreparedPrompt {
    env: SlateEnv,
    input: File,
    files: Vec<File>,
    pub style: PromptStyle,
    pub theme: String,
}

impl PreparedPrompt {
    // The layout browser needs only a bounded theme prerequisite read, not
    // preferences, Starship files, backup storage or any native prompt probe.
    fn read_theme(env: &SlateEnv) -> Result<(File, Option<ThemeVariant>)> {
        let input = File::read(env, env.managed_file("current"), MAX_STATE_BYTES)?;
        let registry = ThemeRegistry::new()?;
        let theme = registry.get(input.text()?.trim()).cloned();
        Ok((input, theme))
    }

    pub(crate) fn has_known_theme(env: &SlateEnv) -> Result<bool> {
        Ok(Self::read_theme(env)?.1.is_some())
    }

    pub(crate) fn capture(env: &SlateEnv, style: PromptStyle) -> Result<Self> {
        let (input, theme) = Self::read_theme(env)?;
        let theme = theme.ok_or_else(|| {
            invalid("save a recognized theme first with `slate theme`; no fallback is assumed")
        })?;
        let mut preferences = File::read(env, env.managed_file("config.toml"), MAX_DOCUMENT_BYTES)?;
        let mut document =
            preference_bytes(preferences.original.as_ref().map(|s| s.bytes.as_slice()))?;
        remember(&mut document, style)?;
        preferences.desired = document.to_string().into_bytes();
        let mut primary = File::read(
            env,
            crate::adapter::StarshipAdapter::integration_config_path_with_env(env),
            MAX_TOOL_CONFIG_BYTES,
        )?;
        primary.desired = style::render(primary.text()?, &theme, style)?.into_bytes();
        let mut fallback = File::read(
            env,
            env.managed_file("managed/starship/plain.toml"),
            MAX_TOOL_CONFIG_BYTES,
        )?;
        fallback.desired = match style {
            PromptStyle::Rainbow => {
                crate::config::shell_integration::themed_plain_starship_content(&theme)
            }
            _ => style::render("", &theme, style)?,
        }
        .into_bytes();
        let files = vec![primary, fallback, preferences]; // Intent last.
        for file in &files {
            if file.desired.len() as u64 > file.limit {
                return Err(invalid("rendered configuration exceeds its size limit"));
            }
        }
        let targets = recovery_paths::targets(env, files.iter().map(|f| f.path.clone()), "Prompt")?;
        if targets.len() != files.len() {
            return Err(invalid("configuration destinations overlap"));
        }
        Ok(Self {
            env: env.clone(),
            input,
            files,
            style,
            theme: theme.id.clone(),
        })
    }
    pub(crate) fn preview(&self) -> PromptPreview {
        PromptPreview {
            schema_version: 1, style: self.style, theme: self.theme.clone(), example: self.style.sample(),
            starship_config_override: self.env.starship_config_override().map(|path| CapturedOverride {
                path: path.to_string_lossy().into_owned(),
                path_is_lossy: path.to_str().is_none(),
                scope: "Captured invocation value only; the override is not resolved, read or added as a write target by this report. Shell startup and live config selection are not verified.",
            }),
            changes: self.files.iter().map(|f| Change { path: f.path.clone(), changed: f.changed() }).collect(),
            notes: vec![
                "Example is illustrative, not a live prompt. Preview runs no Starship or custom commands.",
                "Replaces layout, right prompt, participating module presentation and Slate palette. Other settings and custom command definitions remain; modules outside the preset are not added to its layout.",
                "Updates the standard XDG starship.toml and Slate's plain-font fallback. A custom STARSHIP_CONFIG may override them. No installs, font selection, theme change or shell startup edits.",
                "Starship must already be enabled in the shell. Rainbow uses the plain fallback without a Nerd Font; the other presets use plain symbols by default. Use `slate doctor starship` to inspect the captured config selection and saved preferences without running Starship.",
                "Personal directory substitutions and custom module settings are retained; any icons in those settings may still require your chosen font.",
                "A file recovery point precedes changes. Partial writes are not automatically rolled back. Use the restore point to recover your earlier personal layout.",
            ],
        }
    }
    pub(crate) fn changed(&self) -> bool {
        self.files.iter().any(File::changed)
    }
    pub(crate) fn verify(&self) -> Result<()> {
        self.input.verify(&self.env)?;
        for file in &self.files {
            file.verify(&self.env)?;
        }
        Ok(())
    }
    pub(crate) fn apply(&self) -> Result<Option<String>> {
        self.verify()?;
        let _guard = ConfigWriteGuard::acquire(&self.env)?;
        self.verify()?;
        if !self.changed() {
            return Ok(None);
        }
        let targets = recovery_paths::targets(
            &self.env,
            self.files.iter().map(|f| f.path.clone()),
            "Prompt",
        )?;
        let point = crate::config::snapshot_config_targets_with_env(&self.env, &targets)?;
        let publish = || -> Result<()> {
            self.verify()?;
            for file in &self.files {
                self.input.verify(&self.env)?;
                file.publish(&self.env)?;
            }
            Ok(())
        };
        publish().map_err(|error| SlateError::InvalidConfig(format!(
            "Prompt update incomplete: {error}. Earlier files may have changed; no automatic rollback was attempted. Inspect recovery: slate restore {} --dry-run", point.id
        )))?;
        Ok(Some(point.id))
    }
}

#[cfg(test)]
mod tests;
