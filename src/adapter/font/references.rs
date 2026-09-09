//! Bounded direct-entry reference observations, shared by receipts and doctor.
//! No recursive includes, native processes, saved settings or writes are needed.
use crate::{
    adapter::{AlacrittyAdapter, GhosttyAdapter, KittyAdapter},
    config::file_read::{self, Links, MAX_TOOL_CONFIG_BYTES},
    env::SlateEnv,
    error::{Result, SlateError},
};
use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Terminal {
    Ghostty,
    Alacritty,
    Kitty,
}
impl Terminal {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Ghostty => "Ghostty",
            Self::Alacritty => "Alacritty",
            Self::Kitty => "Kitty",
        }
    }
    pub(crate) fn missing_reason(self) -> &'static str {
        match self {
            Self::Ghostty => "missing Ghostty config",
            Self::Alacritty => "missing alacritty.toml",
            Self::Kitty => "missing kitty.conf",
        }
    }
    pub(crate) fn unlinked_reason(self) -> &'static str {
        match self {
            Self::Alacritty => "no Slate font import found",
            _ => "no Slate font include found",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum State {
    Found,
    NotFound,
    Missing,
    Uninspectable,
}

pub(crate) struct Entry {
    pub path: PathBuf,
    pub state: State,
    pub reason: Option<String>,
}
pub(crate) struct References {
    pub terminal: Terminal,
    pub managed: PathBuf,
    pub entries: Vec<Entry>,
    pub path_error: Option<String>,
}
impl References {
    pub(crate) fn state(&self) -> State {
        if self.entries.iter().any(|entry| entry.state == State::Found) {
            State::Found
        } else if !self.inspection_complete() {
            State::Uninspectable
        } else if self
            .entries
            .iter()
            .any(|entry| entry.state == State::NotFound)
        {
            State::NotFound
        } else {
            State::Missing
        }
    }
    pub(crate) fn inspection_complete(&self) -> bool {
        self.path_error.is_none()
            && self
                .entries
                .iter()
                .all(|entry| entry.state != State::Uninspectable)
    }
}

fn failure(path: &Path, reason: impl std::fmt::Display) -> SlateError {
    SlateError::ConfigReadError(
        path.display().to_string(),
        format!("{reason}; file contents omitted"),
    )
}
fn location(path: &Path) -> Result<PathBuf> {
    match std::fs::canonicalize(path) {
        Ok(path) => Ok(path),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            file_read::directory_alias_target(path)
                .ok_or_else(|| failure(path, "cannot resolve entry location"))
        }
        Err(_) => Err(failure(path, "cannot resolve entry location")),
    }
}

/// Reads regular entry links without authorizing a subsequent write to them.
/// Isolation is checked before reading and location changes are detected after;
/// this is not exclusion of concurrent external editors/directory replacements.
pub(crate) fn inspect_entry(
    env: &SlateEnv,
    path: &Path,
    managed: &Path,
    terminal: Terminal,
) -> Entry {
    let result = (|| -> Result<Option<bool>> {
        let original_location = location(path)?;
        if env.session().is_isolated() && !original_location.starts_with(location(env.home())?) {
            return Err(failure(
                path,
                "entry escapes isolated SLATE_HOME; contents not read",
            ));
        }
        let Some(source) = file_read::read(path, MAX_TOOL_CONFIG_BYTES, Links::Follow)
            .map_err(|error| failure(path, error))?
        else {
            return Ok(None);
        };
        let text =
            std::str::from_utf8(&source.bytes).map_err(|_| failure(path, "entry is not UTF-8"))?;
        if text.contains('\0') {
            return Err(failure(path, "entry contains a NUL byte"));
        }
        let found = match terminal {
            Terminal::Alacritty => crate::adapter::alacritty::integration::inspect_managed_source(
                path, source, managed,
            )?,
            Terminal::Ghostty => {
                use crate::adapter::ghostty::references::{self, Directive};
                for (line, directive) in references::literal_lines(&source.bytes) {
                    if line.strip_suffix(b"\n").unwrap_or(line).len() > 4094 {
                        return Err(failure(
                            path,
                            "entry exceeds the pinned Ghostty 4094-byte line limit",
                        ));
                    }
                    if directive == Directive::Invalid {
                        return Err(failure(path, "invalid config-file reference"));
                    }
                }
                references::contains_literal_reference(&source.bytes, managed)
            }
            Terminal::Kitty => crate::adapter::kitty_config::lines(&source.bytes)
                .any(|line| line.value(b"include") == Some(managed.as_os_str().as_encoded_bytes())),
        };
        if location(path)? != original_location {
            return Err(failure(
                path,
                "entry location changed while inspecting; retry",
            ));
        }
        Ok(Some(found))
    })();
    let (state, reason) = match result {
        Ok(Some(true)) => (State::Found, None),
        Ok(Some(false)) => (State::NotFound, None),
        Ok(None) => (State::Missing, None),
        Err(error) => (State::Uninspectable, Some(error.to_string())),
    };
    Entry {
        path: path.to_owned(),
        state,
        reason,
    }
}

pub(crate) fn inspect(env: &SlateEnv) -> Vec<References> {
    let targets = [
        (
            Terminal::Ghostty,
            "managed/ghostty/font.conf",
            GhosttyAdapter.integration_candidate_paths_with_env(env),
        ),
        (
            Terminal::Alacritty,
            "managed/alacritty/font.toml",
            Ok(vec![AlacrittyAdapter::integration_config_path_with_env(
                env,
            )]),
        ),
        (
            Terminal::Kitty,
            "managed/kitty/font.conf",
            Ok(vec![KittyAdapter::resolve_config_path_with_env(env)]),
        ),
    ];
    targets
        .into_iter()
        .map(|(terminal, relative, paths)| {
            let managed = env.managed_file(relative);
            let (entries, path_error) = match paths {
                Ok(paths) => (
                    paths
                        .into_iter()
                        .map(|path| inspect_entry(env, &path, &managed, terminal))
                        .collect(),
                    None,
                ),
                Err(error) => (Vec::new(), Some(error.to_string())),
            };
            References {
                terminal,
                managed,
                entries,
                path_error,
            }
        })
        .collect()
}
