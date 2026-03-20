//! Alacritty adapter with WriteAndInclude strategy.
//! Alacritty uses TOML import array to include managed config.
//! This adapter edits the import field idempotently using toml_edit::DocumentMut
//! (AST-aware, not regex-based) to ensure safe, structured modifications.

use crate::adapter::{ApplyOutcome, ApplyStrategy, SkipReason, ToolAdapter};
use crate::config::ConfigManager;
use crate::detection;
use crate::env::SlateEnv;
use crate::error::{Result, SlateError};
use crate::theme::ThemeVariant;
use std::fs;
use std::path::{Path, PathBuf};

/// Alacritty adapter implementing the ToolAdapter trait.
pub struct AlacrittyAdapter;

impl AlacrittyAdapter {
    fn config_home_with_env(env: &SlateEnv) -> PathBuf {
        env.xdg_config_home().to_path_buf()
    }

    /// Resolve Alacritty config path, respecting ALACRITTY_SOCKET_PATH and XDG_CONFIG_HOME.
    fn resolve_config_path() -> Result<PathBuf> {
        let env = SlateEnv::from_process()?;
        Ok(Self::resolve_config_path_with_env(&env))
    }

    fn resolve_config_path_with_env(env: &SlateEnv) -> PathBuf {
        let config_home = Self::config_home_with_env(env);

        // Alacritty default: ~/.config/alacritty/alacritty.toml
        config_home.join("alacritty").join("alacritty.toml")
    }

    pub(crate) fn integration_config_path_with_env(env: &SlateEnv) -> PathBuf {
        Self::resolve_config_path_with_env(env)
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
    /// IMPORTANT: This function does NOT create the integration file if it doesn't exist.
    /// The file must already exist (created by setup wizard or user).
    /// This prevents slate from destructively creating a minimal config that could override
    /// system-level settings.
    fn ensure_integration_includes_managed(
        integration_path: &Path,
        managed_path: &Path,
    ) -> Result<()> {
        let managed_str = managed_path.display().to_string();

        // Integration file must already exist; we won't create it implicitly
        if !integration_path.exists() {
            return Ok(());
        }

        // Read existing integration file. Byte-first read + explicit UTF-8
        // check so a stray non-UTF-8 byte in alacritty.toml (issue #3)
        // produces an actionable "which file, where" error instead of a
        // bare IO "stream did not contain valid UTF-8".
        let bytes = fs::read(integration_path).map_err(|e| {
            SlateError::ConfigReadError(integration_path.display().to_string(), e.to_string())
        })?;
        let content = String::from_utf8(bytes).map_err(|e| {
            SlateError::ConfigReadError(
                integration_path.display().to_string(),
                format!(
                    "contains non-UTF-8 bytes at byte offset {} — slate cannot parse this file. \
                     Inspect with `xxd {} | head` around that offset and remove the stray bytes.",
                    e.utf8_error().valid_up_to(),
                    integration_path.display()
                ),
            )
        })?;

        // Parse as TOML AST (preserves comments and formatting)
        let mut doc: toml_edit::DocumentMut = content.parse().map_err(|e| {
            SlateError::InvalidConfig(format!("Failed to parse Alacritty TOML: {}", e))
        })?;

        // Remove [font.normal] from main config if present,
        // since slate manages fonts via the imported colors.toml.
        // Alacritty's main file always overrides imports, so leftover
        // font settings here would shadow our managed values.
        let mut needs_write = false;
        if let Some(font_table) = doc.get_mut("font") {
            if let Some(tbl) = font_table.as_table_mut() {
                if tbl.contains_key("normal") {
                    tbl.remove("normal");
                    needs_write = true;
                }
            }
        }
        // Remove empty [font] table after clearing children
        if doc
            .get("font")
            .and_then(|f| f.as_table())
            .is_some_and(|t| t.is_empty())
        {
            doc.remove("font");
        }

        // Migrate deprecated top-level `import` to `[general] import`
        if doc.get("import").is_some() {
            let old_import = doc.remove("import").unwrap();
            if doc.get("general").is_none() {
                doc["general"] = toml_edit::Item::Table(toml_edit::Table::new());
            }
            if let Some(general) = doc["general"].as_table_mut() {
                general.insert("import", old_import);
            }
            needs_write = true;
        }

        // Get or create [general].import array
        if doc.get("general").is_none() {
            doc["general"] = toml_edit::Item::Table(toml_edit::Table::new());
        }
        if let Some(general) = doc["general"].as_table_mut() {
            if general.get("import").is_none() {
                general.insert(
                    "import",
                    toml_edit::Item::Value(toml_edit::Value::Array(toml_edit::Array::new())),
                );
            }
        }

        let import_array = doc["general"]["import"].as_array_mut().ok_or_else(|| {
            SlateError::InvalidConfig(
                "Alacritty 'general.import' field is not an array".to_string(),
            )
        })?;

        // Idempotent: check if managed path already present
        let already_present = import_array
            .iter()
            .any(|v| v.as_str().is_some_and(|s| s == managed_str));

        if !already_present {
            import_array.push(managed_str);
            needs_write = true;
        }

        if !needs_write {
            return Ok(());
        }

        // Atomic write back to file
        use atomic_write_file::AtomicWriteFile;
        use std::io::Write;
        let mut file = AtomicWriteFile::open(integration_path)?;
        file.write_all(doc.to_string().as_bytes())?;
        file.commit()?;

        Ok(())
    }

    /// Apply font-only update to Alacritty without triggering full theme reapply.
    /// Writes only to dedicated font.toml file (not colors.toml).
    /// Does not touch colors or call theme apply.
    pub fn apply_font_only(env: &SlateEnv, font_name: &str) -> Result<()> {
        let config_manager = ConfigManager::with_env(env)?;
        let integration_path = Self::resolve_config_path_with_env(env);

        // Write only the font section to dedicated font.toml
        let font_content = format!(
            "[font.normal]
family = \"{}\"
",
            font_name
        );
        config_manager.write_managed_file("alacritty", "font.toml", &font_content)?;

        // Ensure integration file includes the font.toml file
        if integration_path.exists() {
            let managed_font_path = config_manager.managed_dir("alacritty").join("font.toml");
            Self::ensure_integration_includes_managed(&integration_path, &managed_font_path)?;
        }

        Ok(())
    }
}

impl ToolAdapter for AlacrittyAdapter {
    fn tool_name(&self) -> &'static str {
        "alacritty"
    }

    fn is_installed(&self) -> Result<bool> {
        Ok(detection::detect_tool_presence(self.tool_name()).installed)
    }

    fn integration_config_path(&self) -> Result<PathBuf> {
        Self::resolve_config_path()
    }

    fn managed_config_path(&self) -> PathBuf {
        let env = SlateEnv::from_process().ok();
        if let Some(env) = env.as_ref() {
            env.config_dir().join("managed").join("alacritty")
        } else {
            PathBuf::from(".config/slate/managed/alacritty")
        }
    }

    fn apply_strategy(&self) -> ApplyStrategy {
        ApplyStrategy::WriteAndInclude
    }

    fn apply_theme(&self, theme: &ThemeVariant) -> Result<ApplyOutcome> {
        let env = SlateEnv::from_process()?;
        self.apply_theme_with_env(theme, &env)
    }

    /// preview-path override. Resolves the integration config and
    /// the managed config directory via the injected `env`, so tempdir-backed
    /// test envs actually influence where Alacritty's managed `colors.toml` /
    /// `opacity.toml` land (previously `apply_theme` called
    /// `SlateEnv::from_process()` internally, making the `&SlateEnv` in
    /// `silent_preview_apply`'s signature a no-op for this adapter).
    fn apply_theme_with_env(&self, theme: &ThemeVariant, env: &SlateEnv) -> Result<ApplyOutcome> {
        let integration_path = Self::resolve_config_path_with_env(env);
        if !integration_path.exists() {
            return Ok(ApplyOutcome::Skipped(SkipReason::MissingIntegrationConfig));
        }

        // Validate theme has palette data
        theme.palette.validate()?;

        // Render theme as TOML color scheme
        let colors_content = Self::render_alacritty_colors(theme);

        // Step 2b: Add font-family — prefer user's saved choice, fallback to detection
        let mut final_colors_content = colors_content;
        let config_mgr = ConfigManager::with_env(env)?;
        let chosen_font = config_mgr.get_current_font().ok().flatten();
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
        config_mgr.write_managed_file("alacritty", "colors.toml", &final_colors_content)?;
        let current_opacity = config_mgr.get_current_opacity_preset()?;
        write_opacity_config(env, current_opacity)?;

        // Ensure integration file includes managed colors path
        let managed_colors_path = config_mgr.managed_dir("alacritty").join("colors.toml");
        let managed_opacity_path = config_mgr.managed_dir("alacritty").join("opacity.toml");

        Self::ensure_integration_includes_managed(&integration_path, &managed_colors_path)?;
        Self::ensure_integration_includes_managed(&integration_path, &managed_opacity_path)?;

        // Touch the main config to trigger Alacritty's live_config_reload,
        // which only watches the main file, not imported files.
        if integration_path.exists() {
            let _ = fs::OpenOptions::new().append(true).open(&integration_path);
        }

        // Alacritty's live_config_reload picks up the new colors in the
        // currently-open window — no new shell required.
        Ok(ApplyOutcome::applied_no_shell())
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
    let config_content = format!(
        "[window]
opacity = {}
",
        opacity_value
    );

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

    /// contract: the trait-level `apply_theme_with_env` must honor
    /// the injected env — managed writes and integration import updates MUST
    /// land inside the tempdir, not the host's real `~/.config/alacritty`.
    #[test]
    fn apply_theme_with_env_honors_injected_env_for_managed_writes() {
        use std::io::Write;
        use tempfile::TempDir;

        let tempdir = TempDir::new().unwrap();
        let env = SlateEnv::with_home(tempdir.path().to_path_buf());
        let adapter = AlacrittyAdapter;

        // Pre-create the Alacritty integration config inside the tempdir so
        // the apply path doesn't early-return with MissingIntegrationConfig.
        let integration_path = AlacrittyAdapter::resolve_config_path_with_env(&env);
        fs::create_dir_all(integration_path.parent().unwrap()).unwrap();
        let mut file = fs::File::create(&integration_path).unwrap();
        writeln!(file, "# slate managed").unwrap();
        drop(file);

        let theme = crate::theme::catppuccin::catppuccin_mocha().unwrap();

        let outcome = ToolAdapter::apply_theme_with_env(&adapter, &theme, &env).unwrap();
        assert!(matches!(outcome, ApplyOutcome::Applied { .. }));

        // Managed writes MUST have landed inside the tempdir.
        let managed_colors = tempdir
            .path()
            .join(".config/slate/managed/alacritty/colors.toml");
        assert!(
            managed_colors.exists(),
            "expected managed colors.toml inside tempdir at {:?}",
            managed_colors
        );

        // Integration import array must reference the tempdir-scoped managed path.
        let integration_content = fs::read_to_string(&integration_path).unwrap();
        assert!(
            integration_content.contains(&managed_colors.display().to_string()),
            "integration config must include the managed colors.toml under tempdir, got:\n{}",
            integration_content
        );
    }
}
