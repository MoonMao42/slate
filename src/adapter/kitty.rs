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
use std::path::{Path, PathBuf};
use std::process::Command;

const KITTY_SOCKET: &str = "unix:/tmp/kitty-slate";

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
        let content = fs::read_to_string(integration_path)?;

        let mut additions = String::new();
        if !content.lines().any(|l| {
            let t = l.trim();
            !t.starts_with('#') && t.starts_with("allow_remote_control")
        }) {
            additions.push_str("allow_remote_control socket-only\n");
        }
        if !content.lines().any(|l| {
            let t = l.trim();
            !t.starts_with('#') && t.starts_with("listen_on")
        }) {
            additions.push_str(&format!("listen_on {}\n", KITTY_SOCKET));
        }

        if additions.is_empty() {
            return Ok(());
        }

        // Prepend so these settings take effect before includes
        let new_content = format!("{}{}", additions, content);
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

        let content = fs::read_to_string(integration_path)?;
        let managed_str = managed_path.display().to_string();

        // Check if already included (line-by-line, skip comments)
        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with('#') || trimmed.is_empty() {
                continue;
            }
            if trimmed.starts_with("include") && trimmed.contains(&managed_str) {
                return Ok(());
            }
        }

        // Append include directive
        let include_line = format!("include {}\n", managed_str);
        let new_content = if content.ends_with('\n') {
            format!("{}{}", content, include_line)
        } else {
            format!("{}\n{}", content, include_line)
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
        let integration_path = Self::resolve_config_path_with_env(&env);

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

        let config_mgr = ConfigManager::with_env(&env)?;

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
        write_opacity_config(&env, current_opacity)?;

        // Ensure integration file includes managed paths
        let managed_base = config_mgr.managed_dir("kitty");
        let theme_path = managed_base.join("theme.conf");
        let opacity_path = managed_base.join("opacity.conf");

        Self::ensure_integration_includes_managed(&integration_path, &theme_path)?;
        Self::ensure_integration_includes_managed(&integration_path, &opacity_path)?;

        Ok(ApplyOutcome::Applied)
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

        // Kitty appends -{pid} to the listen_on path, so we glob for the socket.
        let socket = find_kitty_socket();
        let Some(socket_path) = socket else {
            return Ok(());
        };

        let output = Command::new("kitten")
            .args(["@", "--to", &socket_path, "set-colors", "--all", "--configured"])
            .arg(&theme_path)
            .output();

        match output {
            Ok(o) if o.status.success() => Ok(()),
            // Silent fallback — colors will apply on next Kitty restart
            _ => Ok(()),
        }
    }
}

/// Find the Kitty listen socket. Kitty appends `-{pid}` to the configured
/// `listen_on` path, so we scan `/tmp/` for `kitty-slate-*` entries.
fn find_kitty_socket() -> Option<String> {
    let prefix = "kitty-slate-";
    let entries = fs::read_dir("/tmp").ok()?;
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if name_str.starts_with(prefix) {
            return Some(format!("unix:{}", entry.path().display()));
        }
    }
    None
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::Palette;

    fn create_test_palette() -> Palette {
        Palette {
            foreground: "#ffffff".to_string(),
            background: "#000000".to_string(),
            cursor: None,
            selection_bg: None,
            selection_fg: None,
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
    fn test_reload_succeeds() {
        let adapter = KittyAdapter;
        assert!(adapter.reload().is_ok());
    }
}
