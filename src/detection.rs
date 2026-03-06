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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminalKind {
    Ghostty,
    Kitty,
    Alacritty,
    TerminalApp,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalProfile {
    kind: TerminalKind,
    raw_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalFeatureSummary {
    pub reload: String,
    pub live_preview: String,
    pub font_apply: String,
}

impl TerminalProfile {
    pub fn detect() -> Self {
        let term_program = env::var("TERM_PROGRAM").ok();
        let term = env::var("TERM").ok();
        Self::from_env_vars(term_program.as_deref(), term.as_deref())
    }

    pub fn from_env_vars(term_program: Option<&str>, term: Option<&str>) -> Self {
        let term_program_normalized = term_program.map(|value| value.trim().to_ascii_lowercase());
        let term_normalized = term.map(|value| value.trim().to_ascii_lowercase());

        let kind = match (
            term_program_normalized.as_deref(),
            term_normalized.as_deref(),
        ) {
            (Some("ghostty"), _) | (_, Some("ghostty")) => TerminalKind::Ghostty,
            (Some("kitty"), _) | (_, Some("xterm-kitty")) => TerminalKind::Kitty,
            (Some("alacritty"), _) | (_, Some("alacritty")) => TerminalKind::Alacritty,
            (Some("apple_terminal"), _) => TerminalKind::TerminalApp,
            _ => TerminalKind::Unknown,
        };

        let raw_name = match kind {
            TerminalKind::Ghostty => "Ghostty".to_string(),
            TerminalKind::Kitty => "kitty".to_string(),
            TerminalKind::Alacritty => "Alacritty".to_string(),
            TerminalKind::TerminalApp => "Terminal.app".to_string(),
            TerminalKind::Unknown => term_program
                .or(term)
                .filter(|value| !value.trim().is_empty())
                .unwrap_or("Other terminal")
                .to_string(),
        };

        Self { kind, raw_name }
    }

    pub fn kind(&self) -> TerminalKind {
        self.kind
    }

    pub fn display_name(&self) -> &str {
        &self.raw_name
    }

    pub fn compatibility_label(&self) -> &'static str {
        match self.kind {
            TerminalKind::Ghostty => "best experience",
            TerminalKind::Kitty => "supported",
            TerminalKind::Alacritty => "supported with limits",
            TerminalKind::TerminalApp => "supported with limits",
            TerminalKind::Unknown => "best-effort only",
        }
    }

    pub fn compatibility_summary(&self) -> &'static str {
        match self.kind {
            TerminalKind::Ghostty => {
                "live reload, frosted glass, and watcher relaunch are available"
            }
            TerminalKind::Kitty => {
                "live reload and opacity work, but blur and watcher relaunch stay Ghostty-only"
            }
            TerminalKind::Alacritty => {
                "theme sync works well, but blur and watcher relaunch stay Ghostty-only"
            }
            TerminalKind::TerminalApp => {
                "shell/tool theming works, but fonts stay manual and macOS controls the chrome"
            }
            TerminalKind::Unknown => {
                "core shell/tool theming works, while terminal-specific visuals depend on the app"
            }
        }
    }

    pub fn short_limitations(&self) -> &'static str {
        match self.kind {
            TerminalKind::Ghostty => "live reload, frosted glass, watcher relaunch",
            TerminalKind::Kitty => "live reload, opacity, no blur",
            TerminalKind::Alacritty => "no blur, no watcher relaunch",
            TerminalKind::TerminalApp => "manual font pick, no blur",
            TerminalKind::Unknown => "shell/tool theme only",
        }
    }

    pub fn supports_blur(&self) -> bool {
        matches!(self.kind, TerminalKind::Ghostty)
    }

    pub fn supports_opacity(&self) -> bool {
        matches!(
            self.kind,
            TerminalKind::Ghostty | TerminalKind::Kitty | TerminalKind::Alacritty
        )
    }

    pub fn watcher_shell_autostart_supported(&self) -> bool {
        matches!(self.kind, TerminalKind::Ghostty)
    }

    pub fn font_selection_is_manual(&self) -> bool {
        matches!(self.kind, TerminalKind::TerminalApp | TerminalKind::Unknown)
    }

    pub fn feature_summary(&self) -> TerminalFeatureSummary {
        let reload = match self.kind {
            TerminalKind::Ghostty => {
                if cfg!(target_os = "macos") {
                    "supported via AppleScript".to_string()
                } else {
                    "supported via reload signal".to_string()
                }
            }
            TerminalKind::Kitty => "supported via remote control".to_string(),
            TerminalKind::Alacritty => {
                "best effort via live_config_reload or manual restart".to_string()
            }
            TerminalKind::TerminalApp => "manual restart only".to_string(),
            TerminalKind::Unknown => "unsupported".to_string(),
        };

        let live_preview = match self.kind {
            TerminalKind::Ghostty | TerminalKind::Kitty => "live push supported".to_string(),
            TerminalKind::Alacritty => "inline preview only".to_string(),
            TerminalKind::TerminalApp | TerminalKind::Unknown => "inline preview only".to_string(),
        };

        let font_apply = match self.kind {
            TerminalKind::Ghostty | TerminalKind::Kitty | TerminalKind::Alacritty => {
                "localized config apply supported".to_string()
            }
            TerminalKind::TerminalApp | TerminalKind::Unknown => {
                "manual terminal selection".to_string()
            }
        };

        TerminalFeatureSummary {
            reload,
            live_preview,
            font_apply,
        }
    }

    pub fn setup_review_summary(&self, opacity: Option<f32>, blur_requested: bool) -> String {
        let opacity_label = opacity
            .map(|value| format!("opacity {:.2}", value))
            .unwrap_or_else(|| "core theme sync".to_string());

        match self.kind {
            TerminalKind::Ghostty => {
                if blur_requested {
                    format!("{} · {}, frosted glass", self.display_name(), opacity_label)
                } else {
                    format!("{} · {}", self.display_name(), opacity_label)
                }
            }
            TerminalKind::Kitty | TerminalKind::Alacritty => {
                if blur_requested {
                    format!(
                        "{} · {}, blur not supported here",
                        self.display_name(),
                        opacity_label
                    )
                } else {
                    format!("{} · {}", self.display_name(), opacity_label)
                }
            }
            TerminalKind::TerminalApp => format!(
                "{} · shell/tool theme only, font stays manual",
                self.display_name()
            ),
            TerminalKind::Unknown => {
                format!("{} · shell/tool theme where supported", self.display_name())
            }
        }
    }

    pub fn setup_tip(&self) -> Option<&'static str> {
        match self.kind {
            TerminalKind::Ghostty => None,
            TerminalKind::Kitty => Some(
                "Slate updated Kitty cleanly, but blur and auto-theme relaunch remain Ghostty-only.",
            ),
            TerminalKind::Alacritty => Some(
                "Slate updated Alacritty cleanly, but blur and auto-theme relaunch remain Ghostty-only.",
            ),
            TerminalKind::TerminalApp => Some(
                "Slate themed the shell and tools. Terminal.app still needs a manual Nerd Font pick and does not support frosted backgrounds.",
            ),
            TerminalKind::Unknown => Some(
                "Slate applied the shared shell/tool theme. Terminal-specific visuals depend on this app.",
            ),
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
    candidates.push(PathBuf::from(
        "/usr/share/zsh-syntax-highlighting/zsh-syntax-highlighting.zsh",
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

fn command_aliases(command: &str) -> &'static [&'static str] {
    match command {
        "bat" => &["bat", "batcat"],
        _ => &[],
    }
}

fn command_in_actual_path_with_aliases(command: &str) -> Option<PathBuf> {
    let aliases = command_aliases(command);
    if aliases.is_empty() {
        return command_in_actual_path(command);
    }

    aliases
        .iter()
        .find_map(|candidate| search_paths(candidate, &current_path_dirs()))
}

fn command_path_with_aliases(command: &str, env: &SlateEnv) -> Option<PathBuf> {
    let aliases = command_aliases(command);
    if aliases.is_empty() {
        return command_path_with_env(command, env);
    }

    aliases.iter().find_map(|candidate| {
        search_paths(candidate, &normalized_path_dirs_for_home(Some(env.home())))
    })
}

/// Detect a CLI tool with tier awareness: Tier 1 if in actual PATH, Tier 2 if only in fallback.
fn detect_cli_tool_tiered(command: &str, env: &SlateEnv) -> ToolPresence {
    if let Some(path) = command_in_actual_path_with_aliases(command) {
        ToolPresence::in_path_with(ToolEvidence::Executable(path))
    } else if let Some(path) = command_path_with_aliases(command, env) {
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
        "kitty" => {
            if let Some((path, is_user_local)) = macos_app_path("kitty", env.home()) {
                if is_user_local {
                    ToolPresence::in_path_with(ToolEvidence::AppBundle(path))
                } else {
                    ToolPresence::fallback_with(ToolEvidence::AppBundle(path))
                }
            } else if let Some(path) = command_in_actual_path("kitty") {
                ToolPresence::in_path_with(ToolEvidence::Executable(path))
            } else if let Some(path) = command_path_with_env("kitty", env) {
                ToolPresence::fallback_with(ToolEvidence::Executable(path))
            } else {
                let config = env.xdg_config_home().join("kitty").join("kitty.conf");
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

    #[test]
    fn test_terminal_profile_detects_ghostty() {
        let profile = TerminalProfile::from_env_vars(Some("ghostty"), Some("xterm-256color"));
        assert_eq!(profile.kind(), TerminalKind::Ghostty);
        assert_eq!(profile.display_name(), "Ghostty");
        assert!(profile.supports_blur());
    }

    #[test]
    fn test_terminal_profile_detects_terminal_app() {
        let profile =
            TerminalProfile::from_env_vars(Some("Apple_Terminal"), Some("xterm-256color"));
        assert_eq!(profile.kind(), TerminalKind::TerminalApp);
        assert_eq!(profile.display_name(), "Terminal.app");
        assert!(profile.font_selection_is_manual());
        assert!(!profile.watcher_shell_autostart_supported());
    }

    #[test]
    fn test_terminal_profile_keeps_unknown_name() {
        let profile = TerminalProfile::from_env_vars(Some("WarpTerminal"), Some("xterm-256color"));
        assert_eq!(profile.kind(), TerminalKind::Unknown);
        assert_eq!(profile.display_name(), "WarpTerminal");
        assert_eq!(profile.compatibility_label(), "best-effort only");
    }

    #[test]
    fn test_terminal_feature_summary_for_kitty_mentions_remote_control() {
        let profile = TerminalProfile::from_env_vars(Some("kitty"), Some("xterm-kitty"));
        let summary = profile.feature_summary();

        assert!(summary.reload.contains("remote control"));
        assert!(summary.live_preview.contains("supported"));
        assert!(summary.font_apply.contains("localized"));
    }
}
