//! Kitty adapter with WriteAndInclude strategy.
//! Kitty uses plain-text `.conf` files with `include` directives.
//! Color format: `foreground #RRGGBB`, `color0 #RRGGBB`, etc.
//! Live updates use Kitty remote-control commands when sockets are available.

use crate::adapter::{ApplyOutcome, ApplyStrategy, SkipReason, ToolAdapter};
use crate::config::ConfigManager;
use crate::detection;
use crate::env::SlateEnv;
use crate::error::Result;
use crate::theme::ThemeVariant;
use std::fs;
use std::os::unix::fs::FileTypeExt;
use std::path::{Path, PathBuf};
use std::process::Command;

const KITTY_SOCKET_PREFIX: &str = "kitty-slate-";

/// Resolve the Kitty `listen_on` socket path. Honors `TMPDIR` so macOS sandboxed / multi-user
/// setups pick the per-user temp dir instead of `/tmp`.
fn kitty_socket_dir() -> PathBuf {
    std::env::temp_dir()
}

fn kitty_socket_listen_on() -> String {
    format!("unix:{}/kitty-slate", kitty_socket_dir().display())
}

pub struct KittyAdapter;

impl KittyAdapter {
    pub fn resolve_config_path_with_env(env: &SlateEnv) -> PathBuf {
        env.xdg_config_home().join("kitty").join("kitty.conf")
    }

    /// Ensure kitty.conf has allow_remote_control and listen_on for live preview.
    /// Idempotent: only adds if not already present.
    fn ensure_remote_control(integration_path: &Path) -> Result<()> {
        if !integration_path.exists() {
            return Ok(());
        }
        let content = fs::read(integration_path)?;

        let mut additions = String::new();
        if !super::kitty_config::lines(&content)
            .any(|line| line.value(b"allow_remote_control").is_some())
        {
            additions.push_str("allow_remote_control socket-only\n");
        }
        if !super::kitty_config::lines(&content).any(|line| line.value(b"listen_on").is_some()) {
            additions.push_str(&format!("listen_on {}\n", kitty_socket_listen_on()));
        }
        if !super::kitty_config::lines(&content)
            .any(|line| line.value(b"dynamic_background_opacity").is_some())
        {
            additions.push_str("dynamic_background_opacity yes\n");
        }

        if additions.is_empty() {
            return Ok(());
        }

        // Prepend so these settings take effect before includes
        let new_content = [additions.as_bytes(), content.as_slice()].concat();
        crate::config::preview_write::write_legacy(integration_path, &new_content)?;
        Ok(())
    }

    /// Render Palette into Kitty color config format.
    fn render_kitty_colors(theme: &ThemeVariant) -> String {
        let p = &theme.palette;
        let cursor = p.cursor.as_deref().unwrap_or(&p.foreground);
        let sel_bg = p.selection_bg.as_deref().unwrap_or(&p.bright_black);
        let sel_fg = p.selection_fg.as_deref().unwrap_or(&p.foreground);

        format!(
            "foreground {fg}\n\
             background {bg}\n\
             cursor {cursor}\n\
             cursor_text_color {bg}\n\
             selection_foreground {sel_fg}\n\
             selection_background {sel_bg}\n\
             \n\
             color0 {black}\n\
             color1 {red}\n\
             color2 {green}\n\
             color3 {yellow}\n\
             color4 {blue}\n\
             color5 {magenta}\n\
             color6 {cyan}\n\
             color7 {white}\n\
             \n\
             color8 {br_black}\n\
             color9 {br_red}\n\
             color10 {br_green}\n\
             color11 {br_yellow}\n\
             color12 {br_blue}\n\
             color13 {br_magenta}\n\
             color14 {br_cyan}\n\
             color15 {br_white}\n",
            fg = p.foreground,
            bg = p.background,
            cursor = cursor,
            sel_fg = sel_fg,
            sel_bg = sel_bg,
            black = p.black,
            red = p.red,
            green = p.green,
            yellow = p.yellow,
            blue = p.blue,
            magenta = p.magenta,
            cyan = p.cyan,
            white = p.white,
            br_black = p.bright_black,
            br_red = p.bright_red,
            br_green = p.bright_green,
            br_yellow = p.bright_yellow,
            br_blue = p.bright_blue,
            br_magenta = p.bright_magenta,
            br_cyan = p.bright_cyan,
            br_white = p.bright_white,
        )
    }

    /// Ensure integration file includes managed path via `include` directive (idempotent).
    /// Creates the integration file if it doesn't exist (Kitty doesn't auto-create it).
    /// Kitty syntax: `include /path/to/file.conf`
    fn ensure_integration_includes_managed(
        integration_path: &Path,
        managed_path: &Path,
    ) -> Result<()> {
        if !integration_path.exists() {
            // Don't create here — apply_theme handles initial creation with
            // allow_remote_control settings. Just skip silently.
            return Ok(());
        }

        let content = fs::read(integration_path)?;
        let updated = Self::font_include_content(&content, managed_path);
        if updated != content {
            crate::config::preview_write::write_legacy(integration_path, &updated)?;
        }
        Ok(())
    }

    pub(crate) fn font_include_content(content: &[u8], managed_path: &Path) -> Vec<u8> {
        let managed_str = managed_path.display().to_string();
        let managed_bytes = managed_str.as_bytes();

        // Match the full directive and literal path, including continuations.
        // A path suffix or a differently named key is not an installed include.
        if super::kitty_config::lines(content)
            .any(|line| line.value(b"include") == Some(managed_bytes))
        {
            return content.to_vec();
        }

        // Append include directive
        let include_line = format!("include {}\n", managed_str);
        if content.ends_with(b"\n") {
            [content, include_line.as_bytes()].concat()
        } else {
            [content, b"\n", include_line.as_bytes()].concat()
        }
    }

    /// Apply font-only update to Kitty without triggering full theme reapply.
    pub fn apply_font_only(env: &SlateEnv, font_name: &str) -> Result<()> {
        let font_content = super::font_config::kitty(font_name)?;
        let config_manager = ConfigManager::with_env(env)?;
        let integration_path = Self::resolve_config_path_with_env(env);

        config_manager.write_managed_file("kitty", "font.conf", &font_content)?;

        let managed_font_path = config_manager.managed_dir("kitty").join("font.conf");
        Self::ensure_integration_includes_managed(&integration_path, &managed_font_path)?;

        Ok(())
    }
}

impl ToolAdapter for KittyAdapter {
    fn tool_name(&self) -> &'static str {
        "kitty"
    }

    fn is_installed(&self) -> Result<bool> {
        Ok(detection::detect_tool_presence(self.tool_name()).installed)
    }

    fn is_installed_with_env(&self, env: &SlateEnv) -> Result<bool> {
        Ok(detection::detect_tool_presence_with_env(self.tool_name(), env).installed)
    }

    fn integration_config_path(&self) -> Result<PathBuf> {
        let env = SlateEnv::from_process()?;
        Ok(Self::resolve_config_path_with_env(&env))
    }

    fn managed_config_path(&self) -> PathBuf {
        let env = SlateEnv::from_process().ok();
        if let Some(env) = env.as_ref() {
            env.config_dir().join("managed").join("kitty")
        } else {
            PathBuf::from(".config/slate/managed/kitty")
        }
    }

    fn apply_strategy(&self) -> ApplyStrategy {
        ApplyStrategy::WriteAndInclude
    }

    fn apply_theme(&self, theme: &ThemeVariant) -> Result<ApplyOutcome> {
        let env = SlateEnv::from_process()?;
        self.apply_theme_with_env(theme, &env)
    }

    /// preview-path override. Resolves both the integration config
    /// (`kitty.conf`) and the managed config directory (`managed/kitty/*`) via
    /// the injected `env`, so live-preview callers can drive Kitty through a
    /// tempdir-backed env without any `SlateEnv::from_process()` fallback.
    fn apply_theme_with_env(&self, theme: &ThemeVariant, env: &SlateEnv) -> Result<ApplyOutcome> {
        let integration_path = Self::resolve_config_path_with_env(env);

        // Resolve/serialize the font before creating or editing kitty.conf.
        let config_mgr = ConfigManager::with_env(env)?;
        let chosen_font = config_mgr.get_current_font()?;
        let font_family = chosen_font.or_else(|| {
            crate::adapter::font::FontAdapter::preferred_installed_font_with_env(env)
                .ok()
                .flatten()
        });
        let font_content = font_family
            .as_deref()
            .map(super::font_config::kitty)
            .transpose()?;

        // Kitty doesn't auto-create its config file. If Kitty is installed
        // but kitty.conf is missing, create it so we can add include directives.
        if !integration_path.exists() {
            if let Some(parent) = integration_path.parent() {
                let _ = fs::create_dir_all(parent);
            }
            let _ = crate::config::preview_write::write_legacy(
                &integration_path,
                b"# Created by slate\n",
            );
        }
        if !integration_path.exists() {
            return Ok(ApplyOutcome::Skipped(SkipReason::MissingIntegrationConfig));
        }

        // Ensure remote control is configured for live preview
        let _ = Self::ensure_remote_control(&integration_path);

        theme.palette.validate()?;

        let colors_content = Self::render_kitty_colors(theme);

        // Include font if configured
        let mut final_content = colors_content;
        if let Some(font_content) = font_content {
            final_content = font_content + "\n" + &final_content;
        }

        config_mgr.write_managed_file("kitty", "theme.conf", &final_content)?;

        // Write opacity config
        let current_opacity = config_mgr.get_current_opacity_preset()?;
        write_opacity_config(env, current_opacity)?;

        // Ensure integration file includes managed paths
        let managed_base = config_mgr.managed_dir("kitty");
        let theme_path = managed_base.join("theme.conf");
        let opacity_path = managed_base.join("opacity.conf");

        Self::ensure_integration_includes_managed(&integration_path, &theme_path)?;
        Self::ensure_integration_includes_managed(&integration_path, &opacity_path)?;

        // Kitty pushes colors to running windows via `kitten @ set-colors`
        // in the reload() path; no new shell required to see the change.
        Ok(ApplyOutcome::applied_no_shell())
    }

    fn reload(&self) -> Result<()> {
        let session = crate::session::SessionContext::from_process();
        if session.is_isolated() {
            return Ok(());
        }
        if session.is_remote() {
            return Err(crate::error::SlateError::ReloadFailed(
                "kitty".into(),
                "SSH cannot reload the client terminal".into(),
            ));
        }
        // Kitty does NOT auto-reload included files. Use `kitten @ set-colors`
        // to push colors to all running Kitty windows immediately.
        // Requires `allow_remote_control` in kitty.conf (we add it automatically).
        let env = SlateEnv::from_process()?;
        let config_mgr = ConfigManager::with_env(&env)?;
        let theme_path = config_mgr.managed_dir("kitty").join("theme.conf");

        if !theme_path.exists() {
            return Ok(());
        }

        let sockets = list_kitty_sockets();
        reload_kitty_sockets(
            &config_mgr,
            &sockets,
            |socket_path| {
                kitty_command_succeeded(
                    Command::new("kitten")
                        .args([
                            "@",
                            "--to",
                            socket_path,
                            "set-colors",
                            "--all",
                            "--configured",
                        ])
                        .arg(&theme_path),
                )
            },
            |socket_path, opacity| {
                kitty_command_succeeded(
                    Command::new("kitten")
                        .args(["@", "--to", socket_path, "set-background-opacity", "--all"])
                        .arg(format!("{}", opacity.to_f32())),
                )
            },
        )
    }
}

fn reload_kitty_sockets(
    config: &ConfigManager,
    sockets: &[String],
    colors: impl FnMut(&str) -> bool,
    mut opacity: impl FnMut(&str, crate::opacity::OpacityPreset) -> bool,
) -> Result<()> {
    if sockets.is_empty() {
        return Ok(());
    }
    // Read before either broadcast: an unreadable/invalid preference must not
    // partially update colors and then silently replace opacity with Solid.
    // An absent preference retains ConfigManager's normal Solid default.
    let preset = config.get_current_opacity_preset().map_err(|error| {
        crate::error::SlateError::ReloadFailed(
            "kitty".into(),
            format!("Saved opacity could not be read: {error}. No Kitty update commands were sent. Check with: slate doctor opacity."),
        )
    })?;
    let color_outcome = broadcast_to_kitty_sockets(sockets, colors);
    let opacity_outcome = broadcast_to_kitty_sockets(sockets, |socket| opacity(socket, preset));
    ensure_kitty_reload(&[("colors", color_outcome), ("opacity", opacity_outcome)])
}

/// Reload the original config after restoring preview files, including user
/// overrides and opacity. Unlike set-colors, this also works without managed files.
pub(crate) fn reload_config_after_preview(env: &SlateEnv) -> Result<()> {
    if !env.session().can_reload_terminal() {
        return Ok(());
    }
    let sockets = list_kitty_sockets();
    let result = broadcast_to_kitty_sockets(&sockets, |socket| {
        kitty_command_succeeded(Command::new("kitten").args(["@", "--to", socket, "load-config"]))
    });
    ensure_kitty_reload(&[("restored config", result)])
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum KittyBroadcastOutcome {
    NoSockets,
    AllSucceeded,
    PartialSuccess,
    AllFailed,
}

fn ensure_kitty_reload(outcomes: &[(&str, KittyBroadcastOutcome)]) -> Result<()> {
    let incomplete: Vec<_> = outcomes
        .iter()
        .filter(|(_, outcome)| {
            matches!(
                outcome,
                KittyBroadcastOutcome::PartialSuccess | KittyBroadcastOutcome::AllFailed
            )
        })
        .map(|(stage, _)| *stage)
        .collect();
    if incomplete.is_empty() {
        // No sockets is a quiet no-op, not proof of live activation. Successful
        // command exits likewise do not verify the appearance of a window.
        return Ok(());
    }
    Err(crate::error::SlateError::ReloadFailed(
        "kitty".into(),
        format!(
            "Update not confirmed for every discovered Kitty socket: {}. Reload the config in Kitty. Files were not rolled back.",
            incomplete.join(", ")
        ),
    ))
}

fn kitty_command_succeeded(command: &mut Command) -> bool {
    kitty_command_succeeded_with_limits(
        command,
        crate::platform::process_output::Limits {
            timeout: std::time::Duration::from_secs(2),
            max_output: 16 * 1024,
        },
    )
}

fn kitty_command_succeeded_with_limits(
    command: &mut Command,
    limits: crate::platform::process_output::Limits,
) -> bool {
    use crate::platform::process_output::{capture, Completion};
    // Reuse the owned-process-group capture used by other native adapters.
    // stdin is closed so kitten cannot consume picker input. Deadline/output
    // limits apply per command, not to the complete multi-socket broadcast.
    // Raw native output is deliberately not printed into the interactive UI.
    capture(command, limits).is_ok_and(
        |output| matches!(output.completion, Completion::Exited(status) if status.success()),
    )
}

/// Kitty appends `-{pid}` to the configured `listen_on` path, so we scan the
/// socket directory and sort the discovered sockets for deterministic reloads.
fn list_kitty_sockets() -> Vec<String> {
    list_kitty_sockets_in(&kitty_socket_dir())
}

fn list_kitty_sockets_in(dir: &Path) -> Vec<String> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };

    let mut sockets: Vec<PathBuf> = entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if !name.starts_with(KITTY_SOCKET_PREFIX) {
                return None;
            }

            let Ok(file_type) = entry.file_type() else {
                return None;
            };
            if !file_type.is_socket() {
                return None;
            }

            Some(entry.path())
        })
        .collect();

    sockets.sort_by(|left, right| left.file_name().cmp(&right.file_name()));
    sockets
        .into_iter()
        .map(|path| format!("unix:{}", path.display()))
        .collect()
}

fn broadcast_to_kitty_sockets<F>(sockets: &[String], mut send: F) -> KittyBroadcastOutcome
where
    F: FnMut(&str) -> bool,
{
    if sockets.is_empty() {
        return KittyBroadcastOutcome::NoSockets;
    }

    let mut successful = 0usize;
    for socket in sockets {
        if send(socket) {
            successful += 1;
        }
    }

    if successful == sockets.len() {
        KittyBroadcastOutcome::AllSucceeded
    } else if successful > 0 {
        KittyBroadcastOutcome::PartialSuccess
    } else {
        KittyBroadcastOutcome::AllFailed
    }
}

/// Write opacity configuration to managed Kitty config file.
/// Kitty uses `background_opacity` (0.0 to 1.0).
pub fn write_opacity_config(env: &SlateEnv, opacity: crate::opacity::OpacityPreset) -> Result<()> {
    crate::opacity::ManagedFile::KittyOpacity.write(env, opacity)
}

/// Push opacity to running Kitty via socket (for live preview).
pub fn push_opacity_live(opacity: crate::opacity::OpacityPreset) {
    let _ = try_push_opacity_live(opacity);
}

/// Explicit saved changes report failed refreshes; live previews stay best-effort.
pub(crate) fn try_push_opacity_live(opacity: crate::opacity::OpacityPreset) -> Result<()> {
    if !crate::session::SessionContext::from_process().can_reload_terminal() {
        return Ok(());
    }
    let sockets = list_kitty_sockets();
    let outcome = broadcast_to_kitty_sockets(&sockets, |socket_path| {
        kitty_command_succeeded(
            Command::new("kitten")
                .args(["@", "--to", socket_path, "set-background-opacity", "--all"])
                .arg(format!("{}", opacity.to_f32())),
        )
    });
    ensure_kitty_reload(&[("opacity", outcome)])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::Palette;
    use std::os::unix::net::UnixListener;

    fn create_test_palette() -> Palette {
        Palette {
            foreground: "#ffffff".to_string(),
            background: "#000000".to_string(),
            cursor: None,
            selection_bg: None,
            selection_fg: None,
            brand_accent: "#7287fd".to_string(),
            black: "#000000".to_string(),
            red: "#ff0000".to_string(),
            green: "#00ff00".to_string(),
            yellow: "#ffff00".to_string(),
            blue: "#0000ff".to_string(),
            magenta: "#ff00ff".to_string(),
            cyan: "#00ffff".to_string(),
            white: "#ffffff".to_string(),
            bright_black: "#808080".to_string(),
            bright_red: "#ff6b6b".to_string(),
            bright_green: "#69ff69".to_string(),
            bright_yellow: "#ffff69".to_string(),
            bright_blue: "#6b69ff".to_string(),
            bright_magenta: "#ff69ff".to_string(),
            bright_cyan: "#69ffff".to_string(),
            bright_white: "#ffffff".to_string(),
            rosewater: None,
            flamingo: None,
            pink: None,
            mauve: None,
            lavender: None,
            text: None,
            subtext1: None,
            subtext0: None,
            overlay2: None,
            overlay1: None,
            overlay0: None,
            surface2: None,
            surface1: None,
            surface0: None,
            bg_dim: None,
            bg_darker: None,
            bg_darkest: None,
            extras: std::collections::HashMap::new(),
        }
    }

    fn create_test_theme() -> ThemeVariant {
        ThemeVariant {
            id: "test".to_string(),
            name: "Test Theme".to_string(),
            family: "Test".to_string(),
            palette: create_test_palette(),
            tool_refs: std::collections::HashMap::new(),
            appearance: crate::theme::ThemeAppearance::Dark,
            auto_pair: None,
        }
    }

    #[test]
    fn test_tool_name() {
        let adapter = KittyAdapter;
        assert_eq!(adapter.tool_name(), "kitty");
    }

    #[test]
    fn test_apply_strategy() {
        let adapter = KittyAdapter;
        assert_eq!(adapter.apply_strategy(), ApplyStrategy::WriteAndInclude);
    }

    #[test]
    fn test_render_kitty_colors() {
        let theme = create_test_theme();
        let output = KittyAdapter::render_kitty_colors(&theme);

        assert!(output.contains("foreground #ffffff"));
        assert!(output.contains("background #000000"));
        assert!(output.contains("cursor #ffffff"));
        assert!(output.contains("color0 #000000"));
        assert!(output.contains("color1 #ff0000"));
        assert!(output.contains("color8 #808080"));
        assert!(output.contains("color15 #ffffff"));
        assert!(output.contains("selection_foreground"));
        assert!(output.contains("selection_background"));
    }

    #[test]
    fn test_render_kitty_colors_with_cursor() {
        let mut theme = create_test_theme();
        theme.palette.cursor = Some("#ff0000".to_string());
        let output = KittyAdapter::render_kitty_colors(&theme);
        assert!(output.contains("cursor #ff0000"));
    }

    #[test]
    fn test_ensure_integration_includes_managed_idempotent() {
        use tempfile::NamedTempFile;

        let temp_file = NamedTempFile::new().unwrap();
        let temp_path = temp_file.path().to_path_buf();
        let managed_path = PathBuf::from("/home/user/.config/slate/managed/kitty/theme.conf");

        // First call: should add include line
        KittyAdapter::ensure_integration_includes_managed(&temp_path, &managed_path).unwrap();
        let content1 = fs::read_to_string(&temp_path).unwrap();
        assert!(content1.contains("include /home/user/.config/slate/managed/kitty/theme.conf"));

        // Second call: should be idempotent
        KittyAdapter::ensure_integration_includes_managed(&temp_path, &managed_path).unwrap();
        let content2 = fs::read_to_string(&temp_path).unwrap();
        assert_eq!(content1, content2);
        assert_eq!(content2.matches("include ").count(), 1);
    }

    #[test]
    fn test_ensure_integration_matches_exact_directives_and_continuations() {
        let tempdir = tempfile::TempDir::new().unwrap();
        let file = tempdir.path().join("kitty.conf");
        let managed = tempdir.path().join("managed/theme.conf");
        let expected_line = format!("include {}\n", managed.display());
        for original in [
            format!("include {}.custom\n", managed.display()),
            format!("include_extra {}\n", managed.display()),
            format!("include /mirror{}\n", managed.display()),
            format!("include {}\n\\.custom\n", managed.display()),
            format!("# comment\n\\include {}\n", managed.display()),
        ] {
            fs::write(&file, &original).unwrap();
            KittyAdapter::ensure_integration_includes_managed(&file, &managed).unwrap();
            let expected = format!("{original}{expected_line}");
            assert_eq!(fs::read_to_string(&file).unwrap(), expected);
            KittyAdapter::ensure_integration_includes_managed(&file, &managed).unwrap();
            assert_eq!(fs::read_to_string(&file).unwrap(), expected);
        }
        let original = format!(
            "include\u{2003}{}/\r\n\u{2003}\\theme.conf\r\n",
            managed.parent().unwrap().display()
        );
        fs::write(&file, &original).unwrap();
        let modified = fs::metadata(&file).unwrap().modified().unwrap();
        KittyAdapter::ensure_integration_includes_managed(&file, &managed).unwrap();
        assert_eq!(fs::read_to_string(&file).unwrap(), original);
        assert_eq!(fs::metadata(&file).unwrap().modified().unwrap(), modified);
    }

    #[test]
    fn test_ensure_remote_control_ignores_key_prefix_collisions() {
        let tempdir = tempfile::TempDir::new().unwrap();
        let file = tempdir.path().join("kitty.conf");
        let original = "allow_remote_control_extra yes\nlisten_on_extra unix:/tmp/user\ndynamic_background_opacity_extra yes\n";
        fs::write(&file, original).unwrap();
        KittyAdapter::ensure_remote_control(&file).unwrap();
        let content = fs::read_to_string(&file).unwrap();
        assert!(content.ends_with(original));
        for key in [
            b"allow_remote_control".as_slice(),
            b"listen_on",
            b"dynamic_background_opacity",
        ] {
            assert_eq!(
                super::super::kitty_config::lines(content.as_bytes())
                    .filter(|line| line.value(key).is_some())
                    .count(),
                1
            );
        }
        KittyAdapter::ensure_remote_control(&file).unwrap();
        assert_eq!(fs::read_to_string(&file).unwrap(), content);

        let user_settings = "allow_remote_control no\nlisten_on unix:/tmp/my-socket\ndynamic_background_opacity no\n";
        fs::write(&file, user_settings).unwrap();
        KittyAdapter::ensure_remote_control(&file).unwrap();
        assert_eq!(fs::read_to_string(&file).unwrap(), user_settings);
    }

    #[test]
    fn test_ensure_integration_skips_nonexistent_file() {
        let managed_path = PathBuf::from("/tmp/managed/kitty/theme.conf");
        let nonexistent = PathBuf::from("/tmp/nonexistent/kitty.conf");

        let result = KittyAdapter::ensure_integration_includes_managed(&nonexistent, &managed_path);
        assert!(result.is_ok());
    }

    #[test]
    fn test_ensure_integration_ignores_comments() {
        use std::io::Write;
        use tempfile::TempDir;

        let tempdir = TempDir::new().unwrap();
        let temp_path = tempdir.path().join("kitty.conf");
        let managed_path = PathBuf::from("/home/user/.config/slate/managed/kitty/theme.conf");

        let mut file = fs::File::create(&temp_path).unwrap();
        writeln!(
            file,
            "# include /home/user/.config/slate/managed/kitty/theme.conf"
        )
        .unwrap();
        drop(file);

        // Comment should not count as included
        KittyAdapter::ensure_integration_includes_managed(&temp_path, &managed_path).unwrap();

        let content = fs::read_to_string(&temp_path).unwrap();
        // Should have both the comment and the real include
        assert_eq!(
            content
                .lines()
                .filter(|l| l.starts_with("include "))
                .count(),
            1
        );
    }

    #[test]
    fn test_ensure_integration_preserves_non_utf8_prefix_bytes() {
        use tempfile::TempDir;

        let tempdir = TempDir::new().unwrap();
        let temp_path = tempdir.path().join("kitty.conf");
        let managed_path = PathBuf::from("/home/user/.config/slate/managed/kitty/theme.conf");

        fs::write(&temp_path, [0xff, b'\n']).unwrap();

        KittyAdapter::ensure_integration_includes_managed(&temp_path, &managed_path).unwrap();

        let content = fs::read(&temp_path).unwrap();
        assert!(content.starts_with(&[0xff, b'\n']));
        assert!(
            content.windows(b"include ".len()).any(|w| w == b"include "),
            "managed include line must still be appended"
        );
    }

    #[test]
    fn test_ensure_remote_control_preserves_non_utf8_prefix_bytes() {
        use tempfile::TempDir;

        let tempdir = TempDir::new().unwrap();
        let temp_path = tempdir.path().join("kitty.conf");
        fs::write(&temp_path, [0xff, b'\n']).unwrap();

        KittyAdapter::ensure_remote_control(&temp_path).unwrap();

        let content = fs::read(&temp_path).unwrap();
        assert!(content.starts_with(b"allow_remote_control socket-only\n"));
        assert!(content.windows(1).any(|w| w == [0xff]));
    }

    #[test]
    fn test_list_kitty_sockets_is_stably_sorted() {
        let tempdir = tempfile::TempDir::new().unwrap();
        let b = tempdir.path().join("kitty-slate-200");
        let a = tempdir.path().join("kitty-slate-100");
        let ignored = tempdir.path().join("kitty-slate-not-a-socket");

        let _listener_b = UnixListener::bind(&b).unwrap();
        let _listener_a = UnixListener::bind(&a).unwrap();
        fs::write(&ignored, "not a socket").unwrap();

        let sockets = list_kitty_sockets_in(tempdir.path());
        assert_eq!(
            sockets,
            vec![
                format!("unix:{}", a.display()),
                format!("unix:{}", b.display()),
            ]
        );
    }

    #[test]
    fn test_broadcast_to_kitty_sockets_continues_after_failures() {
        let sockets = vec![
            "unix:/tmp/kitty-slate-2".to_string(),
            "unix:/tmp/kitty-slate-1".to_string(),
        ];
        let mut visited = Vec::new();

        let outcome = broadcast_to_kitty_sockets(&sockets, |socket| {
            visited.push(socket.to_string());
            socket.ends_with("-1")
        });

        assert_eq!(visited, sockets);
        assert_eq!(outcome, KittyBroadcastOutcome::PartialSuccess);
    }

    #[test]
    fn test_broadcast_to_kitty_sockets_is_noop_when_none_found() {
        let mut called = false;

        let outcome = broadcast_to_kitty_sockets(&[], |_| {
            called = true;
            true
        });

        assert!(!called);
        assert_eq!(outcome, KittyBroadcastOutcome::NoSockets);
    }

    #[test]
    fn kitty_reload_reports_every_incomplete_stage_without_retrying() {
        let sockets: Vec<_> = (0..3).map(|i| format!("fixture-{i}")).collect();
        // Cover all success/failure combinations across three sockets, not
        // just "at least one succeeded". Never invoke kitten or real sockets.
        for colors in 0u8..8 {
            for opacity in 0u8..8 {
                let mut outcomes = Vec::new();
                for (stage, successes) in [("colors", colors), ("opacity", opacity)] {
                    let mut visited = Vec::new();
                    let outcome = broadcast_to_kitty_sockets(&sockets, |socket| {
                        let index = visited.len();
                        visited.push(socket.to_owned());
                        successes & (1 << index) != 0
                    });
                    assert_eq!(visited, sockets, "each socket must be tried exactly once");
                    assert_eq!(
                        outcome,
                        match successes {
                            0 => KittyBroadcastOutcome::AllFailed,
                            7 => KittyBroadcastOutcome::AllSucceeded,
                            _ => KittyBroadcastOutcome::PartialSuccess,
                        }
                    );
                    outcomes.push((stage, outcome));
                }
                let result = ensure_kitty_reload(&outcomes);
                assert_eq!(result.is_ok(), colors == 7 && opacity == 7);
                if let Err(error) = result {
                    let message = error.to_string();
                    assert_eq!(message.contains("colors"), colors != 7);
                    assert_eq!(message.contains("opacity"), opacity != 7);
                    assert!(message.contains("Reload the config in Kitty"));
                    assert!(message.contains("Files were not rolled back"));
                }
            }
        }
        assert!(ensure_kitty_reload(&[
            ("colors", KittyBroadcastOutcome::NoSockets),
            ("opacity", KittyBroadcastOutcome::NoSockets),
        ])
        .is_ok());
    }

    #[test]
    fn kitty_preview_restore_reports_partial_reload() {
        for outcome in [
            KittyBroadcastOutcome::PartialSuccess,
            KittyBroadcastOutcome::AllFailed,
        ] {
            let error = ensure_kitty_reload(&[("restored config", outcome)]).unwrap_err();
            assert!(error.to_string().contains("restored config"));
        }
        for outcome in [
            KittyBroadcastOutcome::NoSockets,
            KittyBroadcastOutcome::AllSucceeded,
        ] {
            assert!(ensure_kitty_reload(&[("restored config", outcome)]).is_ok());
        }
    }

    #[test]
    fn kitty_commands_have_bounded_capture_and_cannot_read_picker_input() {
        use crate::platform::process_output::Limits;
        use std::time::Duration;
        for (body, expected) in [
            ("exit 0", true),
            ("exit 7", false),
            ("read value && exit 9; exit 0", true),
            ("while :; do :; done", false),
            ("while :; do printf PRIVATE_OUTPUT; done", false),
        ] {
            let mut command = Command::new("/bin/sh");
            command.env_clear().args(["-c", body]);
            assert_eq!(
                kitty_command_succeeded_with_limits(
                    &mut command,
                    Limits {
                        timeout: Duration::from_millis(200),
                        max_output: 1024,
                    }
                ),
                expected,
                "{body}"
            );
        }
        let home = tempfile::tempdir().unwrap();
        let mut missing = Command::new(home.path().join("nonexistent-kitten"));
        assert!(!kitty_command_succeeded(&mut missing));
    }

    #[test]
    fn kitty_reload_rejects_invalid_opacity_before_sending_any_commands() {
        for fault in ["invalid", "directory", "non-utf8"] {
            let home = tempfile::tempdir().unwrap();
            let env = SlateEnv::with_home(home.path().to_owned());
            let config = ConfigManager::with_env(&env).unwrap();
            let path = env.managed_file("current-opacity");
            match fault {
                "directory" => fs::create_dir(&path).unwrap(),
                "non-utf8" => fs::write(&path, [0xff]).unwrap(),
                _ => fs::write(&path, "not-an-opacity").unwrap(),
            }
            let error = reload_kitty_sockets(
                &config,
                &["fixture".into()],
                |_| panic!("colors sent before validating opacity"),
                |_, _| panic!("invalid opacity must not become Solid"),
            )
            .unwrap_err();
            assert!(error
                .to_string()
                .contains("No Kitty update commands were sent"));
            assert!(error.to_string().contains("slate doctor opacity"));
            if fault == "directory" {
                assert!(path.is_dir());
            } else {
                let expected: &[u8] = if fault == "non-utf8" {
                    &[0xff]
                } else {
                    b"not-an-opacity"
                };
                assert_eq!(fs::read(&path).unwrap(), expected);
            }
            // Without any discovered instance, no read/error or commands are needed.
            reload_kitty_sockets(
                &config,
                &[],
                |_| panic!("no sockets"),
                |_, _| panic!("no sockets"),
            )
            .unwrap();
        }
    }

    #[test]
    fn kitty_reload_uses_saved_opacity_and_keeps_missing_preference_default() {
        use crate::opacity::OpacityPreset;
        for saved in [
            None,
            Some(OpacityPreset::Solid),
            Some(OpacityPreset::Frosted),
            Some(OpacityPreset::Clear),
        ] {
            let home = tempfile::tempdir().unwrap();
            let env = SlateEnv::with_home(home.path().to_owned());
            let config = ConfigManager::with_env(&env).unwrap();
            if let Some(preset) = saved {
                config.set_current_opacity_preset(preset).unwrap();
            }
            let calls = std::cell::RefCell::new(Vec::new());
            reload_kitty_sockets(
                &config,
                &["fixture".into()],
                |socket| {
                    assert_eq!(socket, "fixture");
                    calls.borrow_mut().push("colors");
                    true
                },
                |socket, preset| {
                    assert_eq!(socket, "fixture");
                    assert_eq!(preset, saved.unwrap_or(OpacityPreset::Solid));
                    calls.borrow_mut().push("opacity");
                    true
                },
            )
            .unwrap();
            assert_eq!(*calls.borrow(), ["colors", "opacity"]);
            assert_eq!(
                env.managed_file("current-opacity").exists(),
                saved.is_some()
            );
        }
    }

    /// contract: the trait-level `apply_theme_with_env` must honor
    /// the injected env — managed kitty theme.conf and kitty.conf includes
    /// MUST land inside the tempdir, not the host's `~/.config/kitty`.
    #[test]
    fn apply_theme_with_env_honors_injected_env_for_managed_writes() {
        use tempfile::TempDir;

        let tempdir = TempDir::new().unwrap();
        let env = SlateEnv::with_home(tempdir.path().to_path_buf());
        let adapter = KittyAdapter;

        let theme = create_test_theme();

        // Kitty auto-creates kitty.conf if missing, so we don't need to pre-create.
        let outcome = ToolAdapter::apply_theme_with_env(&adapter, &theme, &env).unwrap();
        assert!(matches!(outcome, ApplyOutcome::Applied { .. }));

        // Managed theme.conf MUST have landed inside the tempdir.
        let managed_theme = tempdir
            .path()
            .join(".config/slate/managed/kitty/theme.conf");
        assert!(
            managed_theme.exists(),
            "expected managed kitty theme.conf inside tempdir at {:?}",
            managed_theme
        );

        // Auto-created kitty.conf must reference the tempdir-scoped managed path.
        let integration_path = KittyAdapter::resolve_config_path_with_env(&env);
        let integration_content = fs::read_to_string(&integration_path).unwrap();
        assert!(
            integration_content.contains(&managed_theme.display().to_string()),
            "kitty.conf must include the managed theme.conf under tempdir, got:\n{}",
            integration_content
        );
    }
}
