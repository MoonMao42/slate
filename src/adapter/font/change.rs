//! Prepare every font/configuration byte before publication; save intent last.
//! File identity checks are not exclusion of external editors or crash atomicity.
use super::super::{font_config, AlacrittyAdapter, GhosttyAdapter, KittyAdapter, ToolAdapter};
use crate::{
    config::{
        file_read::{
            self, Links, Source, MAX_DOCUMENT_BYTES, MAX_STATE_BYTES, MAX_TOOL_CONFIG_BYTES,
        },
        recovery_paths,
        state_files::atomic_write_synced_mode,
        ConfigManager,
    },
    env::SlateEnv,
    error::{Result, SlateError},
};
use std::path::{Path, PathBuf};

fn failure(path: &Path, reason: impl std::fmt::Display) -> SlateError {
    SlateError::ConfigWriteError(
        path.display().to_string(),
        format!("Font change: {reason}; file contents omitted"),
    )
}

struct FileChange {
    path: PathBuf,
    location: PathBuf,
    original: Option<Source>,
    desired: Option<Vec<u8>>,
    limit: u64,
    links: Links,
}
impl FileChange {
    fn capture(env: &SlateEnv, path: PathBuf, limit: u64, writable: bool) -> Result<Self> {
        if writable {
            recovery_paths::validate_file_path(env, &path, "Font")?;
        }
        let links = if writable {
            Links::Reject
        } else {
            Links::Follow
        };
        let original =
            file_read::read(&path, limit, links).map_err(|error| failure(&path, error))?;
        let location = file_read::directory_alias_target(&path)
            .ok_or_else(|| failure(&path, "cannot resolve directory"))?;
        Ok(Self {
            path,
            location,
            original,
            desired: None,
            limit,
            links,
        })
    }
    fn set(&mut self, bytes: Vec<u8>) -> Result<()> {
        if bytes.len() as u64 > self.limit {
            return Err(failure(&self.path, "generated content exceeds file limit"));
        }
        self.desired = Some(bytes);
        Ok(())
    }
    fn changed(&self) -> bool {
        self.desired.as_ref().is_some_and(|bytes| {
            self.original
                .as_ref()
                .is_none_or(|source| &source.bytes != bytes)
        })
    }
    fn verify(&self) -> Result<()> {
        let current = file_read::read(&self.path, self.limit, self.links)
            .map_err(|error| failure(&self.path, error))?;
        if current != self.original
            || file_read::directory_alias_target(&self.path).as_ref() != Some(&self.location)
        {
            return Err(failure(&self.path, "file or directory changed after preparation; retry without overwriting external edits"));
        }
        Ok(())
    }
    fn publish(&self, env: &SlateEnv) -> Result<()> {
        recovery_paths::validate_file_path(env, &self.path, "Font")?;
        self.verify()?;
        if self.changed() {
            std::fs::create_dir_all(
                self.path
                    .parent()
                    .ok_or_else(|| failure(&self.path, "missing parent"))?,
            )?;
            atomic_write_synced_mode(
                &self.path,
                self.desired.as_ref().expect("changed"),
                self.original.as_ref().and_then(|source| source.mode),
            )?;
        }
        Ok(())
    }
}

pub(crate) struct PreparedFont {
    env: SlateEnv,
    inputs: Vec<FileChange>,
    files: Vec<FileChange>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum FontFileAction {
    Create,
    Update,
    Unchanged,
    PreserveAbsent,
}

/// Metadata-only view of the exact prepared publication sequence. Contents are
/// intentionally withheld; preview and apply use the same captured transforms.
pub(crate) struct FontFilePlan {
    pub path: PathBuf,
    pub action: FontFileAction,
    pub before_bytes: Option<usize>,
    pub after_bytes: Option<usize>,
}
impl PreparedFont {
    pub(crate) fn capture(env: &SlateEnv, family: &str) -> Result<Self> {
        font_config::validate_family(family)?;
        let config = ConfigManager::from_env_paths(env);
        let inputs = [
            ("current", MAX_STATE_BYTES),
            ("config.toml", MAX_DOCUMENT_BYTES),
            ("autorun-fastfetch", MAX_STATE_BYTES),
        ]
        .into_iter()
        .map(|(name, limit)| FileChange::capture(env, env.managed_file(name), limit, false))
        .collect::<Result<Vec<_>>>()?;
        let shell = config.font_shell_files(family)?;
        let mut files = Vec::new();
        for (name, contents) in [
            ("ghostty/font.conf", font_config::ghostty(family)?),
            ("alacritty/font.toml", font_config::alacritty(family)?),
            ("kitty/font.conf", font_config::kitty(family)?),
        ] {
            let mut file = FileChange::capture(
                env,
                env.managed_file(&format!("managed/{name}")),
                MAX_TOOL_CONFIG_BYTES,
                true,
            )?;
            file.set(contents.into_bytes())?;
            files.push(file);
        }
        let ghostty_font = env.managed_file("managed/ghostty/font.conf");
        let alacritty_font = env.managed_file("managed/alacritty/font.toml");
        let kitty_font = env.managed_file("managed/kitty/font.conf");
        for path in [&ghostty_font, &alacritty_font, &kitty_font] {
            if path.to_str().is_none_or(|value| {
                value
                    .chars()
                    .any(|c| c.is_control() || matches!(c, '\u{2028}' | '\u{2029}'))
            }) {
                return Err(failure(
                    path,
                    "managed include path must be UTF-8 without control/line separators",
                ));
            }
        }
        let selected = GhosttyAdapter.integration_config_path_with_env(env)?;
        let mut ghostty = Vec::new();
        for path in GhosttyAdapter.integration_candidate_paths_with_env(env)? {
            let mut file = FileChange::capture(env, path, MAX_TOOL_CONFIG_BYTES, true)?;
            if let Some(original) = &file.original {
                let desired = if file.path == selected {
                    GhosttyAdapter::font_include_content(&original.bytes, &ghostty_font)
                } else {
                    GhosttyAdapter::strip_managed_references_from_bytes(
                        &original.bytes,
                        env.managed_file("managed/ghostty")
                            .as_os_str()
                            .as_encoded_bytes(),
                    )
                };
                file.set(desired)?;
            }
            ghostty.push(file);
        }
        // Preserve the existing cleanup-before-primary-reference ordering.
        ghostty.sort_by_key(|file| file.path == selected);
        files.extend(ghostty);
        let mut file = FileChange::capture(
            env,
            AlacrittyAdapter::integration_config_path_with_env(env),
            MAX_TOOL_CONFIG_BYTES,
            true,
        )?;
        if let Some(original) = &file.original {
            file.set(crate::adapter::alacritty::integration::font_content(
                &file.path,
                original,
                &alacritty_font,
            )?)?;
        }
        files.push(file);
        let mut file = FileChange::capture(
            env,
            KittyAdapter::resolve_config_path_with_env(env),
            MAX_TOOL_CONFIG_BYTES,
            true,
        )?;
        if let Some(original) = &file.original {
            file.set(KittyAdapter::font_include_content(
                &original.bytes,
                &kitty_font,
            ))?;
        }
        files.push(file);
        for (tool, name, contents) in shell {
            let mut file = FileChange::capture(
                env,
                env.managed_file(&format!("managed/{tool}/{name}")),
                MAX_TOOL_CONFIG_BYTES,
                true,
            )?;
            file.set(contents.into_bytes())?;
            files.push(file);
        }
        let mut current =
            FileChange::capture(env, env.managed_file("current-font"), MAX_STATE_BYTES, true)?;
        current.set(family.as_bytes().to_vec())?;
        files.push(current); // Commit the saved choice only after required outputs.
        let prepared = Self {
            env: env.clone(),
            inputs,
            files,
        };
        let targets = recovery_paths::targets(env, prepared.paths(), "Font")?;
        if targets.len() != prepared.files.len() {
            return Err(failure(
                env.config_dir(),
                "distinct font targets resolve to the same file; separate their paths",
            ));
        }
        for input in &prepared.inputs {
            if prepared.files.iter().any(|file| {
                file.location == input.location
                    || file
                        .original
                        .as_ref()
                        .zip(input.original.as_ref())
                        .is_some_and(|(output, input)| output.identity == input.identity)
            }) {
                return Err(failure(
                    &input.path,
                    "font output aliases a read-only input; separate their paths",
                ));
            }
        }
        prepared.verify()?;
        Ok(prepared)
    }
    pub(crate) fn paths(&self) -> impl Iterator<Item = PathBuf> + '_ {
        self.files.iter().map(|file| file.path.clone())
    }
    pub(crate) fn file_plan(&self) -> impl Iterator<Item = FontFilePlan> + '_ {
        self.files.iter().map(|file| {
            let before_bytes = file.original.as_ref().map(|source| source.bytes.len());
            let after_bytes = file.desired.as_ref().map(Vec::len).or(before_bytes);
            let action = if file.changed() {
                if before_bytes.is_none() {
                    FontFileAction::Create
                } else {
                    FontFileAction::Update
                }
            } else if before_bytes.is_none() {
                FontFileAction::PreserveAbsent
            } else {
                FontFileAction::Unchanged
            };
            FontFilePlan {
                path: file.path.clone(),
                action,
                before_bytes,
                after_bytes,
            }
        })
    }
    pub(crate) fn changed(&self) -> bool {
        self.files.iter().any(FileChange::changed)
    }
    pub(crate) fn verify(&self) -> Result<()> {
        for file in self.inputs.iter().chain(&self.files) {
            file.verify()?;
        }
        Ok(())
    }
    pub(crate) fn apply(self) -> Result<()> {
        self.apply_with(
            |_| Ok(()),
            |env| {
                let _ = GhosttyAdapter.reload_with_env(env);
            },
        )
    }
    fn apply_with(
        self,
        mut before_file: impl FnMut(usize) -> Result<()>,
        reload: impl FnOnce(&SlateEnv),
    ) -> Result<()> {
        self.verify()?;
        for (index, file) in self.files.iter().enumerate() {
            before_file(index)?;
            for input in &self.inputs {
                input.verify()?;
            }
            file.publish(&self.env)?;
        }
        if self.env.session().can_reload_terminal() {
            reload(&self.env);
        }
        Ok(())
    }
}

#[cfg(all(test, unix))]
#[path = "change_tests.rs"]
mod tests;
