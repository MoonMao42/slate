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
    pub evidence: Option<ToolEvidence>,
}

impl ToolPresence {
    pub fn missing() -> Self {
        Self {
            installed: false,
            evidence: None,
        }
    }

    pub fn installed_with(evidence: ToolEvidence) -> Self {
        Self {
            installed: true,
            evidence: Some(evidence),
        }
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

fn macos_app_path(name: &str, home: &Path) -> Option<PathBuf> {
    [
        PathBuf::from(format!("/Applications/{name}.app")),
        home.join("Applications").join(format!("{name}.app")),
    ]
    .into_iter()
    .find(|path| path.exists())
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

pub fn detect_tool_presence_with_env(tool_id: &str, env: &SlateEnv) -> ToolPresence {
    match tool_id {
        "ghostty" => {
            if let Some(path) = macos_app_path("Ghostty", env.home()) {
                ToolPresence::installed_with(ToolEvidence::AppBundle(path))
            } else if let Some(path) = command_path_with_env("ghostty", env) {
                ToolPresence::installed_with(ToolEvidence::Executable(path))
            } else if let Some(path) = first_existing(ghostty_candidate_paths(env)) {
                ToolPresence::installed_with(ToolEvidence::Config(path))
            } else {
                ToolPresence::missing()
            }
        }
        "alacritty" => {
            if let Some(path) = macos_app_path("Alacritty", env.home()) {
                ToolPresence::installed_with(ToolEvidence::AppBundle(path))
            } else if let Some(path) = command_path_with_env("alacritty", env) {
                ToolPresence::installed_with(ToolEvidence::Executable(path))
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
        "starship" => command_path_with_env("starship", env)
            .map(|path| ToolPresence::installed_with(ToolEvidence::Executable(path)))
            .unwrap_or_else(ToolPresence::missing),
        "bat" => command_path_with_env("bat", env)
            .map(|path| ToolPresence::installed_with(ToolEvidence::Executable(path)))
            .unwrap_or_else(ToolPresence::missing),
        "delta" => command_path_with_env("delta", env)
            .map(|path| ToolPresence::installed_with(ToolEvidence::Executable(path)))
            .unwrap_or_else(ToolPresence::missing),
        "eza" => command_path_with_env("eza", env)
            .map(|path| ToolPresence::installed_with(ToolEvidence::Executable(path)))
            .unwrap_or_else(ToolPresence::missing),
        "lazygit" => command_path_with_env("lazygit", env)
            .map(|path| ToolPresence::installed_with(ToolEvidence::Executable(path)))
            .unwrap_or_else(ToolPresence::missing),
        "fastfetch" => command_path_with_env("fastfetch", env)
            .map(|path| ToolPresence::installed_with(ToolEvidence::Executable(path)))
            .unwrap_or_else(ToolPresence::missing),
        "zsh-syntax-highlighting" => detect_zsh_syntax_highlighting_plugin_with_env(env)
            .map(|path| ToolPresence::installed_with(ToolEvidence::Plugin(path)))
            .unwrap_or_else(ToolPresence::missing),
        "tmux" => command_path_with_env("tmux", env)
            .map(|path| ToolPresence::installed_with(ToolEvidence::Executable(path)))
            .unwrap_or_else(ToolPresence::missing),
        other => command_path_with_env(other, env)
            .map(|path| ToolPresence::installed_with(ToolEvidence::Executable(path)))
            .unwrap_or_else(ToolPresence::missing),
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
