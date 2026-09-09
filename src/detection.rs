use crate::env::SlateEnv;
use std::env;
use std::ffi::{CString, OsString};
use std::os::unix::ffi::OsStrExt;
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
    /// Legacy configuration tier flag. AppBundle, Config and Plugin evidence
    /// also use Tier 1; this flag alone is not executable-in-PATH evidence.
    /// Use `has_path_executable` for user-facing command availability.
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

    /// Is this a Tier 1 configuration candidate? Does not prove live activation.
    pub fn is_tier1(&self) -> bool {
        self.installed && self.in_path
    }

    /// A detected executable in PATH, not a promise that launching it succeeds.
    pub fn has_path_executable(&self) -> bool {
        self.installed && self.in_path && matches!(self.evidence, Some(ToolEvidence::Executable(_)))
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
    session: crate::session::SessionContext,
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
            .with_session(crate::session::SessionContext::from_process())
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

        Self {
            kind,
            raw_name,
            session: crate::session::SessionContext::default(),
        }
    }

    pub fn with_session(mut self, session: crate::session::SessionContext) -> Self {
        if session.is_remote() {
            self.raw_name.push_str(" (SSH)");
        }
        if session.is_multiplexed() && !self.raw_name.starts_with("tmux") {
            self.raw_name.push_str(" (tmux)");
        }
        self.session = session;
        self
    }

    pub fn session(&self) -> &crate::session::SessionContext {
        &self.session
    }

    pub fn kind(&self) -> TerminalKind {
        self.kind
    }

    pub fn display_name(&self) -> &str {
        &self.raw_name
    }

    pub fn compatibility_label(&self) -> &'static str {
        self.compatibility_label_in(crate::config::ui_language::UiLanguage::English)
    }

    pub fn compatibility_label_in(
        &self,
        language: crate::config::ui_language::UiLanguage,
    ) -> &'static str {
        let text = |zh, en| crate::config::ui_language::Text { zh, en }.get(language);
        if self.session.is_remote() {
            return text("远程 Shell", "remote shell");
        }
        if self.session.is_multiplexed() && self.kind == TerminalKind::Unknown {
            return text("tmux 会话", "tmux session");
        }
        match self.kind {
            TerminalKind::Ghostty => text("完整体验", "best experience"),
            TerminalKind::Kitty => text("支持", "supported"),
            TerminalKind::Alacritty => text("支持，但有部分限制", "supported with limits"),
            TerminalKind::TerminalApp => text("支持，但有部分限制", "supported with limits"),
            TerminalKind::Unknown => text("尽力兼容", "best-effort only"),
        }
    }

    pub fn compatibility_summary(&self) -> &'static str {
        if self.session.is_remote() {
            return "themes apply to tools on this host; configure the client terminal's appearance locally";
        }
        if self.session.is_multiplexed() && self.kind == TerminalKind::Unknown {
            return "tmux styles and shell/tool themes are supported; the outer terminal is not identified";
        }
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
        if self.session.is_remote() {
            return "remote tools; client font and opacity stay local";
        }
        if self.session.is_multiplexed() && self.kind == TerminalKind::Unknown {
            return "tmux + shell/tool themes; outer terminal unknown";
        }
        match self.kind {
            TerminalKind::Ghostty => "live reload, frosted glass, watcher relaunch",
            TerminalKind::Kitty => "live reload, opacity, no blur",
            TerminalKind::Alacritty => "no blur, no watcher relaunch",
            TerminalKind::TerminalApp => "manual font pick, no blur",
            TerminalKind::Unknown => "shell/tool theme only",
        }
    }

    pub fn supports_blur(&self) -> bool {
        !self.session.is_remote() && matches!(self.kind, TerminalKind::Ghostty)
    }

    pub fn supports_opacity(&self) -> bool {
        !self.session.is_remote()
            && matches!(
                self.kind,
                TerminalKind::Ghostty | TerminalKind::Kitty | TerminalKind::Alacritty
            )
    }

    pub fn watcher_shell_autostart_supported(&self) -> bool {
        !self.session.is_remote() && matches!(self.kind, TerminalKind::Ghostty)
    }

    pub fn font_selection_is_manual(&self) -> bool {
        self.session.is_remote()
            || matches!(self.kind, TerminalKind::TerminalApp | TerminalKind::Unknown)
    }

    pub fn feature_summary(&self) -> TerminalFeatureSummary {
        if self.session.is_remote() {
            return TerminalFeatureSummary {
                reload: "remote tools only; client terminal is unchanged".into(),
                live_preview: "inline preview only over SSH".into(),
                font_apply: "configure fonts on the client machine".into(),
            };
        }
        if self.session.is_multiplexed() && self.kind == TerminalKind::Unknown {
            return TerminalFeatureSummary {
                reload: "tmux server colors; outer terminal not identified".into(),
                live_preview: "inline preview only".into(),
                font_apply: "configure fonts outside tmux".into(),
            };
        }
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
        self.setup_review_summary_in(
            opacity,
            blur_requested,
            crate::config::ui_language::UiLanguage::English,
        )
    }

    pub fn setup_review_summary_in(
        &self,
        opacity: Option<f32>,
        blur_requested: bool,
        language: crate::config::ui_language::UiLanguage,
    ) -> String {
        let text = |zh, en| crate::config::ui_language::Text { zh, en }.get(language);
        if self.session.is_remote() {
            return format!(
                "{} · {}",
                self.display_name(),
                text(
                    "仅设置远程 Shell/工具配色；客户端外观需在本地设置",
                    "remote shell/tool themes; client appearance stays local"
                )
            );
        }
        let opacity_label = opacity
            .map(|value| format!("{} {:.2}", text("不透明度", "opacity"), value))
            .unwrap_or_else(|| text("同步基础配色", "core theme sync").to_string());

        match self.kind {
            TerminalKind::Ghostty => {
                if blur_requested {
                    format!(
                        "{} · {}, {}",
                        self.display_name(),
                        opacity_label,
                        text("磨砂效果", "frosted glass")
                    )
                } else {
                    format!("{} · {}", self.display_name(), opacity_label)
                }
            }
            TerminalKind::Kitty | TerminalKind::Alacritty => {
                if blur_requested {
                    format!(
                        "{} · {}, {}",
                        self.display_name(),
                        opacity_label,
                        text("此处不支持模糊效果", "blur not supported here")
                    )
                } else {
                    format!("{} · {}", self.display_name(), opacity_label)
                }
            }
            TerminalKind::TerminalApp => format!(
                "{} · {}",
                self.display_name(),
                text(
                    "仅设置 Shell/工具配色，字体需手动设置",
                    "shell/tool theme only, font stays manual"
                )
            ),
            TerminalKind::Unknown => {
                format!(
                    "{} · {}",
                    self.display_name(),
                    text(
                        "仅同步受支持的 Shell/工具配色",
                        "shell/tool theme where supported"
                    )
                )
            }
        }
    }

    pub fn setup_tip(&self) -> Option<&'static str> {
        if self.session.is_remote() {
            return Some("Run slate locally to configure your client terminal's font and opacity.");
        }
        if self.session.is_multiplexed() && self.kind == TerminalKind::Unknown {
            return Some("Run slate outside tmux for terminal-specific appearance controls.");
        }
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
    .find(|path| is_executable_file(path))
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
        .find(|candidate| is_executable_file(candidate))
}

/// Advisory lookup only: follow ordinary symlinks, require a regular file and
/// execute access for effective credentials. Never launch/open the candidate's
/// contents. This does not validate its format/interpreter or prevent later races.
fn is_executable_file(path: &Path) -> bool {
    if !std::fs::metadata(path).is_ok_and(|metadata| metadata.is_file()) {
        return false;
    }
    let Ok(path) = CString::new(path.as_os_str().as_bytes()) else {
        return false;
    };
    // SAFETY: the CString is NUL-terminated and lives throughout this read-only
    // call. AT_FDCWD and AT_EACCESS select the cwd and effective credentials.
    unsafe { libc::faccessat(libc::AT_FDCWD, path.as_ptr(), libc::X_OK, libc::AT_EACCESS) == 0 }
}

pub(crate) struct UnusableCommand {
    pub path: PathBuf,
    pub in_path: bool,
}

/// Call only after ordinary lookup finds no executable. Preserve a rejected
/// candidate for diagnostics without claiming that the tool is installed.
pub(crate) fn unusable_command_with_env(command: &str, env: &SlateEnv) -> Option<UnusableCommand> {
    unusable_command_in_paths(
        command,
        &current_path_dirs(),
        &normalized_path_dirs_for_home(Some(env.home())),
    )
}

fn unusable_command_in_paths(
    command: &str,
    actual: &[PathBuf],
    fallback: &[PathBuf],
) -> Option<UnusableCommand> {
    let aliases = command_aliases(command);
    let names = if aliases.is_empty() {
        std::slice::from_ref(&command)
    } else {
        aliases
    };
    for (paths, in_path) in [(actual, true), (fallback, false)] {
        for name in names {
            for dir in paths {
                let path = dir.join(name);
                // A broken link, directory, FIFO, denied entry or another
                // inspection error is useful evidence; a missing name is not.
                let present = match std::fs::symlink_metadata(&path) {
                    Ok(_) => true,
                    Err(error) => !matches!(
                        error.kind(),
                        std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory
                    ),
                };
                if present {
                    return Some(UnusableCommand { path, in_path });
                }
            }
        }
    }
    None
}

pub fn command_path(command: &str) -> Option<PathBuf> {
    search_paths(command, &normalized_path_dirs())
}

/// GNU `ls` is installed as `gls` by Homebrew's coreutils formula.
/// Used by the macOS preflight BSD-`ls` capability message (D-B1).
pub fn is_gnu_ls_present() -> bool {
    command_path("gls").is_some()
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
    crate::adapter::GhosttyAdapter::config_candidates_with_env(env)
        .unwrap_or_else(|_| crate::adapter::GhosttyAdapter::xdg_config_candidates(env))
        .into_iter()
        .rev()
        .map(|candidate| candidate.path)
        .collect()
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
pub(crate) fn command_in_actual_path(command: &str) -> Option<PathBuf> {
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
                let config =
                    crate::adapter::AlacrittyAdapter::integration_config_path_with_env(env);
                if std::fs::symlink_metadata(&config).is_ok() {
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
        "opencode" => {
            // Check for opencode command in PATH
            if let Some(path) = command_in_actual_path("opencode") {
                ToolPresence::in_path_with(ToolEvidence::Executable(path))
            } else if let Some(path) = command_path_with_env("opencode", env) {
                ToolPresence::fallback_with(ToolEvidence::Executable(path))
            } else if let Some(path) = opencode_config_evidence(env) {
                ToolPresence::installed_with(ToolEvidence::Config(path))
            } else {
                ToolPresence::missing()
            }
        }
        // All other CLI tools: tiered detection
        other => detect_cli_tool_tiered(other, env),
    }
}

// Configuration evidence is not executable detection or config validation.
// Keep it injected and independent of the ambient OPENCODE_TUI_CONFIG value.
fn opencode_config_evidence(env: &SlateEnv) -> Option<PathBuf> {
    if env.opencode_tui_config_error().is_some() {
        return None;
    }
    env.opencode_tui_config()
        .filter(|path| path.exists())
        .map(Path::to_owned)
        .or_else(|| {
            let directory = env.xdg_config_home().join("opencode");
            directory.exists().then_some(directory)
        })
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
    #[test]
    fn executable_lookup_skips_nonexecutable_files_before_valid_candidates() {
        let td = tempfile::tempdir().unwrap();
        let first = td.path().join("first");
        let second = td.path().join("second");
        std::fs::create_dir_all(&first).unwrap();
        std::fs::create_dir_all(&second).unwrap();
        let blocked = first.join("fixture-tool");
        let valid = second.join("fixture-tool");
        std::fs::write(&blocked, "PRIVATE_NONEXECUTABLE").unwrap();
        std::fs::write(&valid, "#!/bin/sh\nexit 99\n").unwrap();
        std::fs::set_permissions(&blocked, std::fs::Permissions::from_mode(0o644)).unwrap();
        std::fs::set_permissions(&valid, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(search_paths("fixture-tool", &[first, second]), Some(valid));
        assert_eq!(
            std::fs::read_to_string(blocked).unwrap(),
            "PRIVATE_NONEXECUTABLE"
        );
    }

    #[test]
    fn executable_lookup_skips_special_and_broken_entries_but_keeps_valid_links() {
        use std::os::unix::fs::symlink;
        let td = tempfile::tempdir().unwrap();
        let mut dirs = Vec::new();
        for name in ["directory", "fifo", "dangling", "loop", "link", "later"] {
            let dir = td.path().join(name);
            std::fs::create_dir(&dir).unwrap();
            dirs.push(dir);
        }
        std::fs::create_dir(dirs[0].join("fixture-tool")).unwrap();
        let fifo = CString::new(dirs[1].join("fixture-tool").as_os_str().as_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(fifo.as_ptr(), 0o755) }, 0);
        symlink(td.path().join("absent"), dirs[2].join("fixture-tool")).unwrap();
        symlink("fixture-tool", dirs[3].join("fixture-tool")).unwrap();
        let target = td.path().join("target");
        std::fs::write(&target, "#!/bin/sh\nexit 99\n").unwrap();
        std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o755)).unwrap();
        let link = dirs[4].join("fixture-tool");
        symlink(&target, &link).unwrap();
        symlink(&target, dirs[5].join("fixture-tool")).unwrap();
        assert_eq!(search_paths("fixture-tool", &dirs), Some(link));
        assert!(search_paths("fixture-tool", &dirs[..4]).is_none());
        let evidence = unusable_command_in_paths("fixture-tool", &dirs[..1], &dirs[1..4]).unwrap();
        assert_eq!(evidence.path, dirs[0].join("fixture-tool"));
        assert!(evidence.in_path);
        let evidence = unusable_command_in_paths("fixture-tool", &[], &dirs[1..4]).unwrap();
        assert_eq!(evidence.path, dirs[1].join("fixture-tool"));
        assert!(!evidence.in_path);
        assert!(unusable_command_in_paths("absent", &dirs, &[]).is_none());
    }

    #[test]
    fn executable_lookup_checks_effective_access_and_preserves_path_bytes() {
        let td = tempfile::tempdir().unwrap();
        let first = td.path().join("first 编辑器");
        let second = td.path().join("second");
        std::fs::create_dir(&first).unwrap();
        std::fs::create_dir(&second).unwrap();
        for dir in [&first, &second] {
            std::fs::write(dir.join("fixture-tool"), "#!/bin/sh\nexit 99\n").unwrap();
        }
        let candidate = first.join("fixture-tool");
        let valid = second.join("fixture-tool");
        std::fs::set_permissions(&candidate, std::fs::Permissions::from_mode(0o001)).unwrap();
        std::fs::set_permissions(&valid, std::fs::Permissions::from_mode(0o755)).unwrap();
        if unsafe { libc::geteuid() } != 0 {
            assert_eq!(
                search_paths("fixture-tool", &[first.clone(), second]),
                Some(valid)
            );
        }
        std::fs::set_permissions(&candidate, std::fs::Permissions::from_mode(0o100)).unwrap();
        assert_eq!(search_paths("fixture-tool", &[first]), Some(candidate));
        let invalid = Path::new(std::ffi::OsStr::from_bytes(b"not-a-path\0suffix"));
        assert!(!is_executable_file(invalid));
    }

    #[test]
    fn ghostty_config_evidence_follows_write_target_in_custom_and_isolated_profiles() {
        let td = tempfile::tempdir().unwrap();
        let home = td.path().join("home");
        let custom = td.path().join("custom-xdg");
        let custom_env = crate::env::SlateEnv::from_vars(|key| match key {
            "HOME" => Some(home.as_os_str().to_owned()),
            "XDG_CONFIG_HOME" => Some(custom.as_os_str().to_owned()),
            _ => None,
        })
        .unwrap();
        let isolated = crate::env::SlateEnv::with_home(home.clone());
        assert_eq!(custom_env.xdg_config_home(), custom);
        assert_eq!(isolated.xdg_config_home(), home.join(".config"));
        for env in [&custom_env, &isolated] {
            let entries = crate::adapter::GhosttyAdapter
                .integration_candidate_paths_with_env(env)
                .unwrap();
            assert_eq!(entries.len(), if cfg!(target_os = "macos") { 4 } else { 2 });
            for path in &entries {
                std::fs::create_dir_all(path.parent().unwrap()).unwrap();
                std::fs::write(path, "# private fixture\n").unwrap();
            }
            // Remove candidates one by one; detection must follow the same
            // last-existing policy, then report no config evidence at all.
            for expected in entries.iter().rev() {
                assert_eq!(
                    super::first_existing(super::ghostty_candidate_paths(env)),
                    Some(expected.clone())
                );
                assert_eq!(
                    crate::adapter::GhosttyAdapter
                        .integration_config_path_with_env(env)
                        .unwrap(),
                    *expected
                );
                std::fs::remove_file(expected).unwrap();
            }
            assert_eq!(
                super::first_existing(super::ghostty_candidate_paths(env)),
                None
            );
            assert_eq!(
                crate::adapter::GhosttyAdapter
                    .integration_config_path_with_env(env)
                    .unwrap(),
                env.xdg_config_home().join("ghostty/config.ghostty")
            );
        }
    }

    #[test]
    fn opencode_config_evidence_uses_only_the_injected_profile() {
        let td = tempfile::tempdir().unwrap();
        let home = td.path().join("profile");
        std::fs::create_dir(&home).unwrap();
        for name in ["first.json", "second.json"] {
            let config = td.path().join(name);
            std::fs::write(&config, "{}").unwrap();
            let env = crate::env::SlateEnv::from_vars(|key| match key {
                "HOME" => Some(home.as_os_str().to_owned()),
                "OPENCODE_TUI_CONFIG" => Some(config.as_os_str().to_owned()),
                _ => None,
            })
            .unwrap();
            assert_eq!(super::opencode_config_evidence(&env), Some(config));
        }
        let isolated = crate::env::SlateEnv::with_home(home.clone());
        assert_eq!(super::opencode_config_evidence(&isolated), None);
        let directory = home.join(".config/opencode");
        std::fs::create_dir_all(&directory).unwrap();
        assert_eq!(super::opencode_config_evidence(&isolated), Some(directory));
    }

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
    fn terminal_review_languages_preserve_capability_limits() {
        use crate::config::ui_language::UiLanguage::{Chinese, English};
        for (program, expected) in [
            ("ghostty", "磨砂效果"),
            ("kitty", "不支持模糊效果"),
            ("Alacritty", "不支持模糊效果"),
            ("Apple_Terminal", "字体需手动设置"),
            ("Other", "受支持的 Shell/工具配色"),
        ] {
            let profile = TerminalProfile::from_env_vars(Some(program), None);
            let zh = profile.setup_review_summary_in(Some(0.85), true, Chinese);
            assert!(zh.contains(expected), "{program}: {zh}");
            assert_eq!(
                profile.setup_review_summary_in(Some(0.85), true, English),
                profile.setup_review_summary(Some(0.85), true)
            );
            assert_eq!(
                profile.compatibility_label_in(English),
                profile.compatibility_label()
            );
        }
        let session = crate::session::SessionContext::from_vars(|key| {
            (key == "SSH_TTY").then(|| "/fixture/tty".into())
        });
        let remote = TerminalProfile::from_env_vars(Some("ghostty"), None).with_session(session);
        let zh = remote.setup_review_summary_in(Some(0.85), true, Chinese);
        assert!(zh.contains("客户端外观需在本地设置"));
        assert!(!zh.contains("磨砂效果"));
        assert_eq!(remote.compatibility_label_in(Chinese), "远程 Shell");
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

    // is_gnu_ls_present helper (LS-03)

    /// Pure-delegation check. `is_gnu_ls_present` must equal `command_path("gls").is_some()`
    /// by construction; any future divergence should fail here loudly.
    #[test]
    fn is_gnu_ls_present_delegates_to_command_path() {
        assert_eq!(is_gnu_ls_present(), command_path("gls").is_some());
    }

    /// Positive path — covered when the dev machine has brew's coreutils installed.
    /// Skipped automatically on hosts without `gls` so CI on bare Linux/macOS images
    /// isn't forced to install coreutils.
    #[test]
    fn is_gnu_ls_present_when_gls_on_path() {
        if command_path("gls").is_none() {
            // No gls on host; positive path is untestable here. Negative path is
            // already covered by the delegation test above.
            return;
        }
        assert!(is_gnu_ls_present());
    }
}
