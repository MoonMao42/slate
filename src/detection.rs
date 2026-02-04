use crate::env::SlateEnv;
use std::env;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolEvidence {
    Executable(PathBuf),
    AppBundle(PathBuf),
    Config(PathBuf),
    Plugin(PathBuf),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolPresence {
    pub installed: bool,
    /// Whether the tool was found in the user's actual PATH (Tier 1) rather than
    /// only in fallback locations like /opt/homebrew/bin (Tier 2).
    /// AppBundle and Config evidence are always considered in-path (user-local).
    pub in_path: bool,
    pub evidence: Option<ToolEvidence>,
}

impl ToolPresence {
    pub fn missing() -> Self {
        Self {
            installed: false,
            in_path: false,
            evidence: None,
        }
    }

    pub fn installed_with(evidence: ToolEvidence) -> Self {
        let in_path = matches!(
            evidence,
            ToolEvidence::AppBundle(_) | ToolEvidence::Config(_) | ToolEvidence::Plugin(_)
        );
        Self {
            installed: true,
            in_path,
            evidence: Some(evidence),
        }
    }

    /// Executable found in user's actual PATH — Tier 1 (active).
    pub fn in_path_with(evidence: ToolEvidence) -> Self {
        Self {
            installed: true,
            in_path: true,
            evidence: Some(evidence),
        }
    }

    /// Executable found only in fallback paths (e.g. /opt/homebrew) — Tier 2 (available).
    pub fn fallback_with(evidence: ToolEvidence) -> Self {
        Self {
            installed: true,
            in_path: false,
            evidence: Some(evidence),
        }
    }

    /// Is this a Tier 1 (active, in PATH) tool?
    pub fn is_tier1(&self) -> bool {
        self.installed && self.in_path
    }
}

fn current_path_dirs() -> Vec<PathBuf> {
    env::var_os("PATH")
        .map(|value| env::split_paths(&value).collect())
        .unwrap_or_default()
}

fn process_home_dir() -> Option<PathBuf> {
    env::var_os("SLATE_HOME")
        .or_else(|| env::var_os("HOME"))
        .map(PathBuf::from)
}

pub fn homebrew_executable() -> Option<PathBuf> {
    [
        PathBuf::from("/opt/homebrew/bin/brew"),
        PathBuf::from("/usr/local/bin/brew"),
    ]
    .into_iter()
    .find(|path| path.is_file())
    .or_else(|| search_paths("brew", &current_path_dirs()))
}

pub fn homebrew_prefix() -> Option<PathBuf> {
    let brew = homebrew_executable()?;
    let parent = brew.parent()?;
    let prefix = parent.parent()?;
    Some(prefix.to_path_buf())
}

fn prepend_unique(paths: &mut Vec<PathBuf>, candidate: PathBuf) {
    if !candidate.as_os_str().is_empty() && !paths.iter().any(|path| path == &candidate) {
        paths.insert(0, candidate);
    }
}

fn normalized_path_dirs_for_home(home: Option<&Path>) -> Vec<PathBuf> {
    let mut paths = current_path_dirs();

    if let Some(prefix) = homebrew_prefix() {
        prepend_unique(&mut paths, prefix.join("sbin"));
        prepend_unique(&mut paths, prefix.join("bin"));
    } else if let Some(brew) = homebrew_executable() {
        if let Some(parent) = brew.parent() {
            prepend_unique(&mut paths, parent.to_path_buf());
        }
    }

    if let Some(home) = home {
        prepend_unique(&mut paths, home.join(".local/bin"));
    }

    paths
}

fn normalized_path_dirs() -> Vec<PathBuf> {
    let process_home = process_home_dir();
    normalized_path_dirs_for_home(process_home.as_deref())
}

pub fn normalized_command_path() -> OsString {
    env::join_paths(normalized_path_dirs())
        .unwrap_or_else(|_| env::var_os("PATH").unwrap_or_default())
}

pub fn apply_normalized_path(command: &mut Command) -> &mut Command {
    command.env("PATH", normalized_command_path())
}

fn search_paths(command: &str, paths: &[PathBuf]) -> Option<PathBuf> {
    paths
        .iter()
        .map(|dir| dir.join(command))
        .find(|candidate| candidate.is_file())
}

pub fn command_path(command: &str) -> Option<PathBuf> {
    search_paths(command, &normalized_path_dirs())
}

pub fn command_path_with_env(command: &str, env: &SlateEnv) -> Option<PathBuf> {
    search_paths(command, &normalized_path_dirs_for_home(Some(env.home())))
}

/// Detect macOS .app bundles. Returns (path, is_user_local).
/// ~/Applications is Tier 1 (user-local); /Applications is Tier 2 (shared system).
fn macos_app_path(name: &str, home: &Path) -> Option<(PathBuf, bool)> {
    // Check user-local first (Tier 1)
    let user_app = home.join("Applications").join(format!("{name}.app"));
    if user_app.exists() {
        return Some((user_app, true));
    }
    // System-wide (Tier 2 — could be installed by another user)
    let system_app = PathBuf::from(format!("/Applications/{name}.app"));
    if system_app.exists() {
        return Some((system_app, false));
    }
    None
}

fn ghostty_candidate_paths(env: &SlateEnv) -> Vec<PathBuf> {
    let mut paths = vec![
        env.xdg_config_home().join("ghostty").join("config.ghostty"),
        env.xdg_config_home().join("ghostty").join("config"),
    ];

    paths.push(
        env.home()
            .join("Library/Application Support/com.mitchellh.ghostty/config.ghostty"),
    );
    paths.push(
        env.home()
            .join("Library/Application Support/com.mitchellh.ghostty/config"),
    );

    paths
}

fn first_existing(paths: impl IntoIterator<Item = PathBuf>) -> Option<PathBuf> {
    paths.into_iter().find(|path| path.exists())
}

pub fn detect_zsh_syntax_highlighting_plugin(home: &Path) -> Option<PathBuf> {
    let mut candidates = Vec::new();

    if let Some(prefix) = homebrew_prefix() {
        candidates.push(prefix.join("share/zsh-syntax-highlighting/zsh-syntax-highlighting.zsh"));
    }

    candidates.push(PathBuf::from(
        "/opt/homebrew/share/zsh-syntax-highlighting/zsh-syntax-highlighting.zsh",
    ));
    candidates.push(PathBuf::from(
        "/usr/local/share/zsh-syntax-highlighting/zsh-syntax-highlighting.zsh",
    ));
    candidates
        .push(home.join(".oh-my-zsh/plugins/zsh-syntax-highlighting/zsh-syntax-highlighting.zsh"));
    candidates.push(home.join(".zsh/zsh-syntax-highlighting/zsh-syntax-highlighting.zsh"));

    first_existing(candidates)
}

pub fn detect_zsh_syntax_highlighting_plugin_with_env(env: &SlateEnv) -> Option<PathBuf> {
    detect_zsh_syntax_highlighting_plugin(env.home())
}

pub fn detect_tool_presence(tool_id: &str) -> ToolPresence {
    SlateEnv::from_process()
        .map(|env| detect_tool_presence_with_env(tool_id, &env))
        .unwrap_or_else(|_| ToolPresence::missing())
}

/// Check if a command exists in the user's actual PATH (without fallback dirs).
fn command_in_actual_path(command: &str) -> Option<PathBuf> {
    search_paths(command, &current_path_dirs())
}

/// Detect a CLI tool with tier awareness: Tier 1 if in actual PATH, Tier 2 if only in fallback.
fn detect_cli_tool_tiered(command: &str, env: &SlateEnv) -> ToolPresence {
    if let Some(path) = command_in_actual_path(command) {
        ToolPresence::in_path_with(ToolEvidence::Executable(path))
    } else if let Some(path) = command_path_with_env(command, env) {
        ToolPresence::fallback_with(ToolEvidence::Executable(path))
    } else {
        ToolPresence::missing()
    }
}

pub fn detect_tool_presence_with_env(tool_id: &str, env: &SlateEnv) -> ToolPresence {
    match tool_id {
        "ghostty" => {
            if let Some((path, is_user_local)) = macos_app_path("Ghostty", env.home()) {
                if is_user_local {
                    ToolPresence::in_path_with(ToolEvidence::AppBundle(path))
                } else {
                    ToolPresence::fallback_with(ToolEvidence::AppBundle(path))
                }
            } else if let Some(path) = command_in_actual_path("ghostty") {
                ToolPresence::in_path_with(ToolEvidence::Executable(path))
            } else if let Some(path) = command_path_with_env("ghostty", env) {
                ToolPresence::fallback_with(ToolEvidence::Executable(path))
            } else if let Some(path) = first_existing(ghostty_candidate_paths(env)) {
                ToolPresence::installed_with(ToolEvidence::Config(path))
            } else {
                ToolPresence::missing()
            }
        }
        "alacritty" => {
            if let Some((path, is_user_local)) = macos_app_path("Alacritty", env.home()) {
                if is_user_local {
                    ToolPresence::in_path_with(ToolEvidence::AppBundle(path))
                } else {
                    ToolPresence::fallback_with(ToolEvidence::AppBundle(path))
                }
            } else if let Some(path) = command_in_actual_path("alacritty") {
                ToolPresence::in_path_with(ToolEvidence::Executable(path))
            } else if let Some(path) = command_path_with_env("alacritty", env) {
                ToolPresence::fallback_with(ToolEvidence::Executable(path))
            } else {
                let config = env
                    .xdg_config_home()
                    .join("alacritty")
                    .join("alacritty.toml");
                if config.exists() {
                    ToolPresence::installed_with(ToolEvidence::Config(config))
                } else {
                    ToolPresence::missing()
                }
            }
        }
        "zsh-syntax-highlighting" => detect_zsh_syntax_highlighting_plugin_with_env(env)
            .map(|path| ToolPresence::installed_with(ToolEvidence::Plugin(path)))
            .unwrap_or_else(ToolPresence::missing),
        // All other CLI tools: tiered detection
        other => detect_cli_tool_tiered(other, env),
    }
}

pub fn shell_quote(value: &str) -> String {
    if value.is_empty() {
        return "''".to_string();
    }

    let escaped = value.replace('\'', "'\"'\"'");
    format!("'{}'", escaped)
}

pub fn shell_quote_path(path: &Path) -> String {
    shell_quote(&path.to_string_lossy())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;
    use tempfile::TempDir;

    #[test]
    fn test_shell_quote_neutralizes_command_substitution() {
        let quoted = shell_quote("/tmp/$(touch boom)");
        assert_eq!(quoted, "'/tmp/$(touch boom)'");
    }

    #[test]
    fn test_detect_zsh_highlighting_plugin_prefers_homebrew_share() {
        let temp = TempDir::new().unwrap();
        let env = SlateEnv::with_home(temp.path().to_path_buf());
        let path = detect_zsh_syntax_highlighting_plugin_with_env(&env);

        if let Some(found) = path {
            assert!(found.to_string_lossy().contains("zsh-syntax-highlighting"));
        }
    }

    #[test]
    fn test_command_path_with_env_finds_user_local_bin() {
        let temp = TempDir::new().unwrap();
        let env = SlateEnv::with_home(temp.path().to_path_buf());
        let local_bin = env.user_local_bin();
        fs::create_dir_all(&local_bin).unwrap();

        let executable = local_bin.join("starship");
        fs::write(&executable, "#!/bin/sh\nexit 0\n").unwrap();
        #[cfg(unix)]
        {
            let mut permissions = fs::metadata(&executable).unwrap().permissions();
            permissions.set_mode(0o755);
            fs::set_permissions(&executable, permissions).unwrap();
        }

        let detected = command_path_with_env("starship", &env);
        assert_eq!(detected.as_deref(), Some(executable.as_path()));
    }
}
