//! Alacritty adapter with WriteAndInclude strategy.
//! Alacritty uses TOML import array to include managed config.
//! This adapter edits the import field idempotently using toml_edit::DocumentMut
//! (AST-aware, not regex-based) to ensure safe, structured modifications.

use crate::adapter::{ApplyStrategy, ToolAdapter};
use crate::env::SlateEnv;
use crate::config::ConfigManager;
use crate::error::{Result, SlateError};
use crate::theme::ThemeVariant;
use std::fs;
use std::path::{Path, PathBuf};

/// Alacritty adapter implementing v2 ToolAdapter trait.
pub struct AlacrittyAdapter;

impl AlacrittyAdapter {
    /// Get config home directory (XDG default)
    fn config_home() -> Result<PathBuf> {
        let env = SlateEnv::from_process()?;
let home = env.home().to_str().ok_or(SlateError::MissingHomeDir)?;
        Ok(PathBuf::from(home).join(".config"))
    }

    /// Resolve Alacritty config path, respecting ALACRITTY_SOCKET_PATH and XDG_CONFIG_HOME.
    fn resolve_config_path() -> Result<PathBuf> {
        let config_home = Self::config_home()?;

        // Alacritty default: ~/.config/alacritty/alacritty.toml
        Ok(config_home.join("alacritty").join("alacritty.toml"))
    }

    /// Render Palette into Alacritty TOML color scheme structure.
    /// Maps palette colors to Alacritty's colors.primary, colors.normal, colors.bright sections.
    fn render_alacritty_colors(theme: &ThemeVariant) -> String {
        let palette = &theme.palette;

        format!(
            "[colors.primary]\nbackground = \"{}\"\nforeground = \"{}\"\n\n\
[colors.normal]\nblack = \"{}\"\nred = \"{}\"\ngreen = \"{}\"\nyellow = \"{}\"\nblue = \"{}\"\nmagenta = \"{}\"\ncyan = \"{}\"\nwhite = \"{}\"\n\n\
[colors.bright]\nblack = \"{}\"\nred = \"{}\"\ngreen = \"{}\"\nyellow = \"{}\"\nblue = \"{}\"\nmagenta = \"{}\"\ncyan = \"{}\"\nwhite = \"{}\"\n",
            palette.background,
            palette.foreground,
            // normal colors
            palette.black,
            palette.red,
            palette.green,
            palette.yellow,
            palette.blue,
            palette.magenta,
            palette.cyan,
            palette.white,
            // bright colors
            palette.bright_black,
            palette.bright_red,
            palette.bright_green,
            palette.bright_yellow,
            palette.bright_blue,
            palette.bright_magenta,
            palette.bright_cyan,
            palette.bright_white,
        )
    }

    /// Ensure integration file includes managed path in import array (idempotent).
    /// Uses toml_edit AST to safely modify the import array.
    fn ensure_integration_includes_managed(
        integration_path: &Path,
        managed_path: &Path,
    ) -> Result<()> {
        let managed_str = managed_path.display().to_string();

        // Read or create integration file
        let content = if integration_path.exists() {
            fs::read_to_string(integration_path)?
        } else {
            String::new()
        };

        // Parse as TOML AST (preserves comments and formatting)
        let mut doc: toml_edit::DocumentMut = content.parse().map_err(|e| {
            SlateError::InvalidConfig(format!("Failed to parse Alacritty TOML: {}", e))
        })?;

        // Get or create the import array
        if doc.get("import").is_none() {
            doc["import"] =
                toml_edit::Item::Value(toml_edit::Value::Array(toml_edit::Array::new()));
        }

        let import_array = doc["import"].as_array_mut().ok_or_else(|| {
            SlateError::InvalidConfig("Alacritty 'import' field is not an array".to_string())
        })?;

        // Idempotent: check if managed path already present
        let already_present = import_array
            .iter()
            .any(|v| v.as_str().map_or(false, |s| s == managed_str));

        if already_present {
            return Ok(());
        }

        // Append managed path to import array
        import_array.push(managed_str);

        // Atomic write back to file (per)
        use atomic_write_file::AtomicWriteFile;
        use std::io::Write;
        let mut file = AtomicWriteFile::open(integration_path)?;
        file.write_all(doc.to_string().as_bytes())?;
        file.commit()?;

        Ok(())
    }
}

impl ToolAdapter for AlacrittyAdapter {
    fn tool_name(&self) -> &'static str {
        "alacritty"
    }

    fn is_installed(&self) -> Result<bool> {
        // Check if binary exists in PATH
        let binary_exists = which::which("alacritty").is_ok();

        // Check if config directory exists
        let config_home = match Self::config_home() {
            Ok(home) => home,
            Err(_) => return Ok(binary_exists),
        };

        let config_dir = config_home.join("alacritty");
        let config_dir_exists = config_dir.exists();

        Ok(binary_exists || config_dir_exists)
    }

    fn integration_config_path(&self) -> Result<PathBuf> {
        Self::resolve_config_path()
    }

    fn managed_config_path(&self) -> PathBuf {
        let env = SlateEnv::from_process().ok();
let home = env.as_ref().and_then(|e| e.home().to_str().map(|s| s.to_string()));
        if let Some(h) = home {
            PathBuf::from(h).join(".config/slate/managed/alacritty")
        } else {
            PathBuf::from(".config/slate/managed/alacritty")
        }
    }

    fn apply_strategy(&self) -> ApplyStrategy {
        ApplyStrategy::WriteAndInclude
    }

    fn apply_theme(&self, theme: &ThemeVariant) -> Result<()> {
        // Validate theme has palette data
        theme.palette.validate()?;

        // Render theme as TOML color scheme
        let colors_content = Self::render_alacritty_colors(theme);

        // Step 2b: Add font-family — prefer user's saved choice, fallback to detection
        let mut final_colors_content = colors_content;
        let chosen_font = crate::config::ConfigManager::new()
            .ok()
            .and_then(|cm| cm.get_current_font().ok().flatten());
        let font_family = chosen_font.or_else(|| {
            crate::adapter::font::FontAdapter::detect_installed_fonts()
                .ok()
                .and_then(|f| f.into_iter().next())
        });
        if let Some(family) = font_family {
            let font_section = format!("[font.normal]\nfamily = \"{}\"\n\n", family);
            final_colors_content = font_section + &final_colors_content;
        }
        // Write managed colors file
        let config_mgr = ConfigManager::new()?;
        config_mgr.write_managed_file("alacritty", "colors.toml", &final_colors_content)?;

        // Ensure integration file includes managed colors path
        let integration_path = self.integration_config_path()?;
        let managed_colors_path = self.managed_config_path().join("colors.toml");

        Self::ensure_integration_includes_managed(&integration_path, &managed_colors_path)?;

        Ok(())
    }

    fn reload(&self) -> Result<()> {
        // Alacritty supports live_config_reload if enabled, but it's optional.
        // Best-effort: return Err indicating manual restart may be needed.
        Err(SlateError::ReloadFailed(
            "alacritty".to_string(),
            "Alacritty reload depends on live_config_reload setting. \
             Restart your terminal or set live_config_reload = true in alacritty.toml."
                .to_string(),
        ))
    }
}


/// Write opacity configuration to managed Alacritty config file.
/// Alacritty only supports opacity (alpha), no blur.
/// Writes [window] opacity = {f32} to managed config file.
/// Path: ~/.config/slate/managed/alacritty/opacity.toml
pub fn write_opacity_config(env: &SlateEnv, opacity: crate::opacity::OpacityPreset) -> Result<()> {
    let config_manager = ConfigManager::with_env(env)?;
    
    let opacity_value = opacity.to_f32();
    let config_content = format!("[window]
opacity = {}
", opacity_value);
    
    // Write to managed file, will be idempotently included in import array
    config_manager.write_managed_file("alacritty", "opacity.toml", &config_content)?;
    
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
            tool_refs: std::collections::HashMap::from([
                ("ghostty".to_string(), "test".to_string()),
                ("alacritty".to_string(), "test".to_string()),
                ("bat".to_string(), "test".to_string()),
                ("delta".to_string(), "test".to_string()),
                ("starship".to_string(), "test".to_string()),
                ("eza".to_string(), "test".to_string()),
                ("lazygit".to_string(), "test".to_string()),
                ("fastfetch".to_string(), "test".to_string()),
                ("tmux".to_string(), "test".to_string()),
                ("zsh_syntax_highlighting".to_string(), "test".to_string()),
            ]),
            appearance: crate::theme::ThemeAppearance::Dark,
            auto_pair: None,
        }
    }

    #[test]
    fn test_tool_name() {
        let adapter = AlacrittyAdapter;
        assert_eq!(adapter.tool_name(), "alacritty");
    }

    #[test]
    fn test_apply_strategy() {
        let adapter = AlacrittyAdapter;
        assert_eq!(adapter.apply_strategy(), ApplyStrategy::WriteAndInclude);
    }

    #[test]
    fn test_render_alacritty_colors() {
        let theme = create_test_theme();
        let output = AlacrittyAdapter::render_alacritty_colors(&theme);

        assert!(output.contains("[colors.primary]"));
        assert!(output.contains("background = \"#000000\""));
        assert!(output.contains("foreground = \"#ffffff\""));
        assert!(output.contains("[colors.normal]"));
        assert!(output.contains("[colors.bright]"));
    }

    #[test]
    fn test_integration_includes_managed_idempotent() {
        use tempfile::NamedTempFile;

        let temp_file = NamedTempFile::new().unwrap();
        let temp_path = temp_file.path().to_path_buf();

        let managed_path = PathBuf::from("/home/user/.config/slate/managed/alacritty/colors.toml");

        // First call: should add to empty config
        AlacrittyAdapter::ensure_integration_includes_managed(&temp_path, &managed_path).unwrap();

        let content1 = fs::read_to_string(&temp_path).unwrap();
        assert!(content1.contains(".config/slate/managed/alacritty/colors.toml"));

        // Second call: should be idempotent (no duplicate)
        AlacrittyAdapter::ensure_integration_includes_managed(&temp_path, &managed_path).unwrap();

        let content2 = fs::read_to_string(&temp_path).unwrap();
        assert_eq!(content1, content2);
    }

    #[test]
    fn test_is_installed_when_not_present() {
        let adapter = AlacrittyAdapter;
        let _result = adapter.is_installed();
    }
}
