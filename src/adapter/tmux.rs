//! tmux adapter with marker block system for .tmux.conf theming.
//! tmux uses source-file directive to include managed config.
//! This adapter uses the MarkerBlock module for safe, validated editing.
//! Detects tmux installation but doesn't require it (optional tool).

use crate::adapter::{marker_block, ApplyOutcome, ApplyStrategy, ToolAdapter};
use crate::detection;
use crate::env::SlateEnv;
use crate::error::{Result, SlateError};
use crate::theme::ThemeVariant;
use std::path::{Path, PathBuf};
use std::process::Command;

/// tmux adapter implementing the ToolAdapter trait.
pub struct TmuxAdapter;

impl TmuxAdapter {
    /// Active default config entry: legacy home path, then XDG candidates.
    fn tmux_conf_path() -> Result<PathBuf> {
        let env = SlateEnv::from_process()?;
        Ok(Self::tmux_conf_path_with_env(&env))
    }

    fn tmux_conf_path_with_env(env: &SlateEnv) -> PathBuf {
        env.tmux_config_path()
    }

    /// Render tmux status bar color configuration
    /// Maps palette to 7 tmux color elements:
    /// 1. status-style (bg/fg)
    /// 2. window-status-current-style (bg/fg)
    /// 3. pane-border-style (fg)
    /// 4. pane-active-border-style (fg)
    /// 5. message-style (bg/fg)
    /// 6. mode-style (bg/fg)
    /// 7. message-command-style (bg/fg)
    pub fn render_tmux_colors(theme: &ThemeVariant) -> String {
        let palette = &theme.palette;
        let active_foreground = active_window_foreground(theme);
        let mode_foreground = readable_foreground(theme, &palette.black, &palette.blue);

        format!(
            "# tmux status bar colors managed by slate\n\
             set -g status-style \"bg={} fg={}\"\n\
             set -g window-status-current-style \"bg={} fg={} bold\"\n\
             set -g pane-border-style \"fg={}\"\n\
             set -g pane-active-border-style \"fg={}\"\n\
             set -g message-style \"bg={} fg={}\"\n\
             set -g mode-style \"bg={} fg={}\"\n\
             set -g message-command-style \"bg={} fg={}\"\n",
            palette.background, // status bg
            palette.foreground, // status fg
            palette.blue,       // active window bg (accent)
            active_foreground,  // active window fg, readable on accent
            palette.black,      // inactive pane fg (muted)
            palette.blue,       // active pane fg (accent)
            palette.background, // message bg
            palette.foreground, // message fg
            palette.black,      // mode selection bg (muted)
            mode_foreground,    // readable mode selection fg
            palette.background, // message-command bg
            palette.foreground  // message-command fg
        )
    }

    /// Render managed block with source-file directive
    fn render_tmux_block(managed_path: &Path) -> Result<String> {
        let managed_str = detection::shell_quote(&literal_source_path(managed_path)?);
        Ok(format!(
            "{}\nsource-file {}\n{}\n",
            marker_block::START,
            managed_str,
            marker_block::END
        ))
    }
}

fn active_window_foreground(theme: &ThemeVariant) -> &str {
    readable_foreground(theme, &theme.palette.blue, &theme.palette.foreground)
}

fn readable_foreground<'a>(
    theme: &'a ThemeVariant,
    background: &str,
    preferred: &'a str,
) -> &'a str {
    let palette = &theme.palette;
    // Keep the original foreground when it is readable. Prefer the theme's
    // background for inversion, then palette neutrals before a black/white fallback.
    for color in [
        preferred,
        &palette.foreground,
        &palette.background,
        &palette.black,
        &palette.white,
        &palette.bright_black,
        &palette.bright_white,
    ] {
        if crate::wcag::contrast_hex(color, background) >= 4.5 {
            return color;
        }
    }
    if crate::wcag::contrast_hex("#000000", background) >= 4.5 {
        "#000000"
    } else {
        "#ffffff"
    }
}

impl ToolAdapter for TmuxAdapter {
    fn tool_name(&self) -> &'static str {
        "tmux"
    }

    fn is_installed(&self) -> Result<bool> {
        self.is_installed_with_env(&SlateEnv::from_process()?)
    }

    fn is_installed_with_env(&self, env: &SlateEnv) -> Result<bool> {
        Ok(detection::detect_tool_presence_with_env(self.tool_name(), env).installed)
    }

    fn integration_config_path(&self) -> Result<PathBuf> {
        Self::tmux_conf_path()
    }

    fn managed_config_path(&self) -> PathBuf {
        let env = SlateEnv::from_process().ok();
        if let Some(env) = env.as_ref() {
            env.config_dir().join("managed").join("tmux")
        } else {
            PathBuf::from(".config/slate/managed/tmux")
        }
    }

    fn apply_strategy(&self) -> ApplyStrategy {
        ApplyStrategy::WriteAndInclude
    }

    fn apply_theme(&self, theme: &ThemeVariant) -> Result<ApplyOutcome> {
        let env = SlateEnv::from_process()?;
        self.apply_theme_with_env(theme, &env)
    }

    fn apply_theme_with_env(&self, theme: &ThemeVariant, env: &SlateEnv) -> Result<ApplyOutcome> {
        // Validate theme has palette data
        theme.palette.validate()?;

        // Render tmux color configuration
        let tmux_colors = Self::render_tmux_colors(theme);

        let tmux_conf_path = Self::tmux_conf_path_with_env(env);
        crate::config::recovery_paths::validate_file_path(env, &tmux_conf_path, "tmux")?;
        let destination =
            super::integration_publish::destination(&tmux_conf_path, "tmux configuration")?;
        let original = crate::config::file_read::read(
            &tmux_conf_path,
            crate::config::file_read::MAX_TOOL_CONFIG_BYTES,
            crate::config::file_read::Links::Reject,
        )
        .map_err(|_| {
            SlateError::InvalidConfig(
                "Cannot safely read tmux configuration; no files were changed.".into(),
            )
        })?;
        let managed_colors_path = env.managed_file("managed/tmux/colors.conf");
        let new_block = Self::render_tmux_block(&managed_colors_path)?;
        let updated = marker_block::upsert_managed_block_bytes(
            original
                .as_ref()
                .map_or(&[], |source| source.bytes.as_slice()),
            new_block.as_bytes(),
        )?;
        if updated.len() as u64 > crate::config::file_read::MAX_TOOL_CONFIG_BYTES {
            return Err(SlateError::InvalidConfig(
                "Adding tmux's theme include would exceed 8 MiB; no files were changed.".into(),
            ));
        }
        // A single adapter must not initialize unrelated profile state.
        super::managed_fragment::write(env, &managed_colors_path, tmux_colors.as_bytes(), "tmux")?;
        super::integration_publish::publish(
            env,
            &tmux_conf_path,
            &destination,
            original.as_ref(),
            &updated,
            "tmux configuration",
        )?;

        // tmux source-file is issued in reload() against the running server
        // so existing sessions pick up the new colors immediately.
        Ok(ApplyOutcome::applied_no_shell())
    }

    fn reload(&self) -> Result<()> {
        self.reload_with_env(&SlateEnv::from_process()?)
    }

    fn reload_with_env(&self, env: &SlateEnv) -> Result<()> {
        if env.session().is_isolated() {
            return Ok(());
        }
        if env.session().is_multiplexed() && env.session().tmux_socket().is_none() {
            return Err(SlateError::ReloadFailed(
                "tmux".into(),
                "Cannot resolve the current TMUX socket".into(),
            ));
        }
        let presence = detection::detect_tool_presence_with_env("tmux", env);
        let Some(detection::ToolEvidence::Executable(binary)) = presence.evidence else {
            return Err(SlateError::ReloadFailed(
                "tmux".into(),
                "Colors saved; no tmux executable was detected for reload.".into(),
            ));
        };
        let mut command = Command::new(std::path::absolute(binary)?);
        // A theme change must not start a server or execute the user's startup
        // commands again. Load only Slate's generated color settings.
        command.arg("-N").env_remove("TMUX").env("LC_ALL", "C");
        if let Some(socket) = env.session().tmux_socket() {
            command.arg("-S").arg(socket);
        }
        command.arg("source-file").arg(literal_source_path(
            &env.config_dir().join("managed/tmux/colors.conf"),
        )?);
        run_reload(
            &mut command,
            crate::platform::process_output::Limits {
                timeout: std::time::Duration::from_secs(3),
                max_output: 64 * 1024,
            },
            env.session().tmux_socket().is_none(),
        )
    }
}

fn run_reload(
    command: &mut Command,
    limits: crate::platform::process_output::Limits,
    default_server: bool,
) -> Result<()> {
    use crate::platform::process_output::{self, Completion};
    let failure = |reason: String| {
        SlateError::ReloadFailed("tmux".into(), format!(
        "Colors saved; server reload was not confirmed: {reason}. Some options may already have applied. Inspect the target tmux session before retrying; native output omitted."
    ))
    };
    let output = process_output::capture(command, limits)
        .map_err(|error| failure(format!("could not run tmux ({})", error.kind())))?;
    match output.completion {
        Completion::Exited(status) if status.success() => Ok(()),
        Completion::Exited(status)
            if default_server
                && status.code() == Some(1)
                && missing_default_socket(&output.stdout, &output.stderr) =>
        {
            Err(SlateError::NoDefaultTmuxServer)
        }
        Completion::Exited(status) => Err(failure(format!("tmux exited with {status}"))),
        Completion::TimedOut => Err(failure(format!(
            "exceeded {} ms post-spawn deadline",
            limits.timeout.as_millis()
        ))),
        Completion::OutputLimit => Err(failure(format!(
            "exceeded {} byte output limit",
            limits.max_output
        ))),
    }
}

// Only the native connection error for a missing default socket is expected
// inactivity. Permissions, stale sockets, custom targets and config errors must
// retain failure reporting. LC_ALL=C pins the diagnostic emitted by tmux.
fn missing_default_socket(stdout: &[u8], stderr: &[u8]) -> bool {
    if !stdout.is_empty() {
        return false;
    }
    let Ok(text) = std::str::from_utf8(stderr) else {
        return false;
    };
    text.trim()
        .strip_prefix("error connecting to ")
        .and_then(|text| text.strip_suffix(" (No such file or directory)"))
        .is_some_and(|path| path.starts_with('/') && !path.chars().any(char::is_control))
}

/// source-file expands glob patterns even when passed as a single argv item.
fn literal_source_path(path: &Path) -> Result<String> {
    let path = path.to_str().filter(|value| !value.chars().any(char::is_control))
        .ok_or_else(|| SlateError::InvalidConfig("tmux theme path must be UTF-8 without control characters; cannot safely generate or execute source-file.".into()))?;
    let mut literal = String::new();
    for ch in path.chars() {
        if matches!(ch, '\\' | '*' | '?' | '[' | ']') {
            literal.push('\\');
        }
        literal.push(ch);
    }
    Ok(literal)
}

#[cfg(test)]
mod tests;
