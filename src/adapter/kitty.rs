//! Kitty adapter with WriteAndInclude strategy.
//! Kitty uses plain-text `.conf` files with `include` directives.
//! Color format: `foreground #RRGGBB`, `color0 #RRGGBB`, etc.
//! Config auto-reloads on file change (no signal needed).

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
    fn trim_ascii(bytes: &[u8]) -> &[u8] {
        let start = bytes
            .iter()
            .position(|b| !b.is_ascii_whitespace())
            .unwrap_or(bytes.len());
        let end = bytes
            .iter()
            .rposition(|b| !b.is_ascii_whitespace())
            .map(|idx| idx + 1)
            .unwrap_or(start);
        &bytes[start..end]
    }

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
        if !content.split(|b| *b == b'\n').any(|line| {
            let t = Self::trim_ascii(line);
            !t.starts_with(b"#") && t.starts_with(b"allow_remote_control")
        }) {
            additions.push_str("allow_remote_control socket-only\n");
        }
        if !content.split(|b| *b == b'\n').any(|line| {
            let t = Self::trim_ascii(line);
            !t.starts_with(b"#") && t.starts_with(b"listen_on")
        }) {
            additions.push_str(&format!("listen_on {}\n", kitty_socket_listen_on()));
        }
        if !content.split(|b| *b == b'\n').any(|line| {
            let t = Self::trim_ascii(line);
            !t.starts_with(b"#") && t.starts_with(b"dynamic_background_opacity")
        }) {
            additions.push_str("dynamic_background_opacity yes\n");
        }

        if additions.is_empty() {
            return Ok(());
        }

        // Prepend so these settings take effect before includes
        let new_content = [additions.as_bytes(), content.as_slice()].concat();
        fs::write(integration_path, new_content)?;
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
        let managed_str = managed_path.display().to_string();
        let managed_bytes = managed_str.as_bytes();

        // Check if already included (line-by-line, skip comments)
        for line in content.split(|b| *b == b'\n') {
            let trimmed = Self::trim_ascii(line);
            if trimmed.starts_with(b"#") || trimmed.is_empty() {
                continue;
            }
            if trimmed.starts_with(b"include")
                && trimmed
                    .windows(managed_bytes.len())
                    .any(|w| w == managed_bytes)
            {
                return Ok(());
            }
        }

        // Append include directive
        let include_line = format!("include {}\n", managed_str);
        let new_content = if content.ends_with(b"\n") {
            [content.as_slice(), include_line.as_bytes()].concat()
        } else {
            [content.as_slice(), b"\n", include_line.as_bytes()].concat()
        };
        fs::write(integration_path, new_content)?;

        Ok(())
    }

    /// Apply font-only update to Kitty without triggering full theme reapply.
    pub fn apply_font_only(env: &SlateEnv, font_name: &str) -> Result<()> {
        let config_manager = ConfigManager::with_env(env)?;
        let integration_path = Self::resolve_config_path_with_env(env);

        let font_content = format!("font_family {}\n", font_name);
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

        // Kitty doesn't auto-create its config file. If Kitty is installed
        // but kitty.conf is missing, create it so we can add include directives.
        if !integration_path.exists() {
            if let Some(parent) = integration_path.parent() {
                let _ = fs::create_dir_all(parent);
            }
            let _ = fs::write(&integration_path, "# Created by slate\n");
        }
        if !integration_path.exists() {
            return Ok(ApplyOutcome::Skipped(SkipReason::MissingIntegrationConfig));
        }

        // Ensure remote control is configured for live preview
        let _ = Self::ensure_remote_control(&integration_path);

        theme.palette.validate()?;

        let colors_content = Self::render_kitty_colors(theme);

        let config_mgr = ConfigManager::with_env(env)?;

        // Include font if configured
        let mut final_content = colors_content;
        let chosen_font = config_mgr.get_current_font().ok().flatten();
        let font_family = chosen_font.or_else(|| {
            crate::adapter::font::FontAdapter::detect_installed_fonts()
                .ok()
                .and_then(|f| f.into_iter().next())
        });
        if let Some(family) = font_family {
            final_content = format!("font_family {}\n\n{}", family, final_content);
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
        let color_outcome = broadcast_to_kitty_sockets(&sockets, |socket_path| {
            Command::new("kitten")
                .args([
                    "@",
                    "--to",
                    socket_path,
                    "set-colors",
                    "--all",
                    "--configured",
                ])
                .arg(&theme_path)
                .output()
                .map(|output| output.status.success())
                .unwrap_or(false)
        });

        let opacity = config_mgr
            .get_current_opacity_preset()
            .unwrap_or(crate::opacity::OpacityPreset::Solid);
        let opacity_outcome = broadcast_to_kitty_sockets(&sockets, |socket_path| {
            Command::new("kitten")
                .args(["@", "--to", socket_path, "set-background-opacity", "--all"])
                .arg(format!("{}", opacity.to_f32()))
                .output()
                .map(|output| output.status.success())
                .unwrap_or(false)
        });

        if matches!(color_outcome, KittyBroadcastOutcome::AllFailed)
            && matches!(opacity_outcome, KittyBroadcastOutcome::AllFailed)
        {
            return Err(crate::error::SlateError::Internal(
                "Failed to reload any running Kitty instance".to_string(),
            ));
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum KittyBroadcastOutcome {
    NoSockets,
    PartialSuccess,
    AllFailed,
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

    if successful > 0 {
        KittyBroadcastOutcome::PartialSuccess
    } else {
        KittyBroadcastOutcome::AllFailed
    }
}

/// Write opacity configuration to managed Kitty config file.
/// Kitty uses `background_opacity` (0.0 to 1.0).
pub fn write_opacity_config(env: &SlateEnv, opacity: crate::opacity::OpacityPreset) -> Result<()> {
    let config_manager = ConfigManager::with_env(env)?;

    let opacity_value = opacity.to_f32();
    let config_content = format!("background_opacity {}\n", opacity_value);

    config_manager.write_managed_file("kitty", "opacity.conf", &config_content)?;

    Ok(())
}

/// Push opacity to running Kitty via socket (for live preview).
pub fn push_opacity_live(opacity: crate::opacity::OpacityPreset) {
    let sockets = list_kitty_sockets();
    let _ = broadcast_to_kitty_sockets(&sockets, |socket_path| {
        Command::new("kitten")
            .args(["@", "--to", socket_path, "set-background-opacity", "--all"])
            .arg(format!("{}", opacity.to_f32()))
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    });
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
