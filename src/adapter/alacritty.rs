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
use std::path::PathBuf;
#[cfg(test)]
use std::{fs, path::Path};

pub(crate) mod integration;
mod paths;

/// Alacritty adapter implementing the ToolAdapter trait.
pub struct AlacrittyAdapter;

impl AlacrittyAdapter {
    /// Resolve the first user-level TOML candidate. A socket path is not a
    /// configuration override; per-process --config paths are not inferred.
    fn resolve_config_path() -> Result<PathBuf> {
        let env = SlateEnv::from_process()?;
        Ok(Self::resolve_config_path_with_env(&env))
    }

    fn resolve_config_path_with_env(env: &SlateEnv) -> PathBuf {
        paths::resolve(env)
    }

    pub(crate) fn integration_candidate_paths_with_env(env: &SlateEnv) -> Vec<PathBuf> {
        paths::candidates(env)
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

    #[cfg(test)]
    fn ensure_integration_includes_managed(
        integration_path: &Path,
        managed_path: &Path,
    ) -> Result<()> {
        if let Some(document) = integration::Document::read(integration_path)? {
            document
                .prepare(&[managed_path.to_owned()], true)?
                .publish()?;
        }
        Ok(())
    }

    /// Apply font-only update to Alacritty without triggering full theme reapply.
    /// Writes only to dedicated font.toml file (not colors.toml).
    /// Does not touch colors or call theme apply.
    pub fn apply_font_only(env: &SlateEnv, font_name: &str) -> Result<()> {
        let font_content = super::font_config::alacritty(font_name)?;
        let config_manager = ConfigManager::from_env_paths(env);
        let integration_path = Self::resolve_config_path_with_env(env);
        let managed_font_path = config_manager.managed_dir("alacritty").join("font.toml");
        let prepared = integration::Document::read(&integration_path)?
            .map(|document| document.prepare(&[managed_font_path], true))
            .transpose()?;
        if let Some(prepared) = &prepared {
            prepared.verify()?;
        }

        // Write only the font section to dedicated font.toml
        config_manager.write_managed_file("alacritty", "font.toml", &font_content)?;

        if let Some(prepared) = prepared {
            prepared.publish()?;
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

    fn is_installed_with_env(&self, env: &SlateEnv) -> Result<bool> {
        Ok(detection::detect_tool_presence_with_env(self.tool_name(), env).installed)
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
        let Some(document) = integration::Document::read(&integration_path)? else {
            return Ok(ApplyOutcome::Skipped(SkipReason::MissingIntegrationConfig));
        };

        // Validate theme has palette data
        theme.palette.validate()?;

        // Render theme as TOML color scheme
        let colors_content = Self::render_alacritty_colors(theme);

        // Step 2b: Add font-family — prefer user's saved choice, fallback to detection
        let mut final_colors_content = colors_content;
        let config_mgr = ConfigManager::from_env_paths(env);
        let chosen_font = config_mgr.get_current_font()?;
        let font_family = chosen_font.or_else(|| {
            crate::adapter::font::FontAdapter::preferred_installed_font_with_env(env)
                .ok()
                .flatten()
        });
        let has_managed_font = font_family.is_some();
        if let Some(family) = font_family {
            let font_section = super::font_config::alacritty(&family)?;
            final_colors_content = font_section + "\n" + &final_colors_content;
        }
        let current_opacity = config_mgr.get_current_opacity_preset()?;
        let managed_colors_path = config_mgr.managed_dir("alacritty").join("colors.toml");
        let managed_opacity_path = config_mgr.managed_dir("alacritty").join("opacity.toml");
        let prepared = document.prepare(
            &[managed_colors_path, managed_opacity_path],
            has_managed_font,
        )?;
        prepared.verify()?;

        // All local inputs are validated before the first managed write. These
        // publications are individually atomic, not a multi-file transaction.
        config_mgr.write_managed_file("alacritty", "colors.toml", &final_colors_content)?;
        write_opacity_config(env, current_opacity)?;
        prepared.publish()?;

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
    crate::opacity::ManagedFile::AlacrittyOpacity.write(env, opacity)
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
    fn integration_preserves_both_import_locations_and_their_order() {
        let td = tempfile::tempdir().unwrap();
        let path = td.path().join("alacritty.toml");
        let managed = td.path().join("colors.toml");
        fs::write(&path, "# legacy list\nimport = ['base.toml', 'override.toml']\n[general]\n# deliberately shadowed list\nimport = ['inactive.toml']\nlive_config_reload = false\n").unwrap();
        AlacrittyAdapter::ensure_integration_includes_managed(&path, &managed).unwrap();
        let text = fs::read_to_string(&path).unwrap();
        let doc: toml_edit::DocumentMut = text.parse().unwrap();
        assert_eq!(
            doc.get("import")
                .unwrap()
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap())
                .collect::<Vec<_>>(),
            vec!["base.toml", "override.toml", managed.to_str().unwrap()]
        );
        assert_eq!(doc["general"]["import"][0].as_str(), Some("inactive.toml"));
        assert_eq!(doc["general"]["live_config_reload"].as_bool(), Some(false));
        assert!(text.contains("# legacy list"));
        assert!(text.contains("# deliberately shadowed list"));
    }

    #[test]
    fn alacritty_resolution_reuses_native_user_paths_in_precedence_order() {
        let td = tempfile::tempdir().unwrap();
        let home = td.path().join("home");
        let xdg = td.path().join("custom-config");
        fs::create_dir_all(&home).unwrap();
        let env = SlateEnv::from_vars(|key| match key {
            "HOME" => Some(home.clone().into_os_string()),
            "XDG_CONFIG_HOME" => Some(xdg.clone().into_os_string()),
            _ => None,
        })
        .unwrap();
        let paths = [
            xdg.join("alacritty/alacritty.toml"),
            xdg.join("alacritty.toml"),
            home.join(".config/alacritty/alacritty.toml"),
            home.join(".alacritty.toml"),
        ];
        assert_eq!(
            AlacrittyAdapter::resolve_config_path_with_env(&env),
            paths[0]
        );
        for path in paths.iter().rev() {
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, "# user\n").unwrap();
            assert_eq!(AlacrittyAdapter::resolve_config_path_with_env(&env), *path);
        }
    }

    #[test]
    fn alacritty_resolution_deduplicates_directory_aliases_and_keeps_obstructions() {
        use std::os::unix::fs::symlink;
        let td = tempfile::tempdir().unwrap();
        let env = SlateEnv::with_home(td.path().to_owned());
        assert_eq!(
            AlacrittyAdapter::integration_candidate_paths_with_env(&env).len(),
            3
        );
        let default = env.xdg_config_home().join("alacritty/alacritty.toml");
        fs::create_dir_all(default.parent().unwrap()).unwrap();
        fs::write(env.home().join(".alacritty.toml"), "# lower priority\n").unwrap();
        let missing = env.home().join("missing-target");
        symlink(&missing, &default).unwrap();
        assert_eq!(
            AlacrittyAdapter::resolve_config_path_with_env(&env),
            default
        );
        assert!(integration::Document::read(&default).is_err());
        assert!(!missing.exists());

        let alias = env.home().join("xdg-alias");
        symlink(env.xdg_config_home(), &alias).unwrap();
        let aliased = SlateEnv::from_vars(|key| match key {
            "HOME" => Some(env.home().as_os_str().to_owned()),
            "XDG_CONFIG_HOME" => Some(alias.clone().into_os_string()),
            _ => None,
        })
        .unwrap();
        let paths = AlacrittyAdapter::integration_candidate_paths_with_env(&aliased);
        assert_eq!(
            paths.len(),
            3,
            "directory aliases must not duplicate snapshot targets"
        );
        assert_eq!(paths[0], alias.join("alacritty/alacritty.toml"));
        assert_eq!(
            AlacrittyAdapter::resolve_config_path_with_env(&aliased),
            paths[0]
        );
    }

    #[test]
    fn integration_supports_inline_general_without_losing_imports() {
        let td = tempfile::tempdir().unwrap();
        let path = td.path().join("alacritty.toml");
        let managed = td.path().join("colors.toml");
        fs::write(
            &path,
            "general = { import = ['user.toml'], live_config_reload = false }\n",
        )
        .unwrap();
        AlacrittyAdapter::ensure_integration_includes_managed(&path, &managed).unwrap();
        let doc: toml_edit::DocumentMut = fs::read_to_string(&path).unwrap().parse().unwrap();
        assert_eq!(doc["general"]["import"][0].as_str(), Some("user.toml"));
        assert_eq!(doc["general"]["import"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn invalid_integration_fails_before_managed_output_and_omits_source() {
        let td = tempfile::tempdir().unwrap();
        let env = SlateEnv::with_home(td.path().to_owned());
        let path = AlacrittyAdapter::resolve_config_path_with_env(&env);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "private_key = 'PRIVATE_CONTENT'\n[broken TOML").unwrap();
        let error = AlacrittyAdapter
            .apply_theme_with_env(&create_test_theme(), &env)
            .unwrap_err()
            .to_string();
        assert!(
            !env.config_dir().exists(),
            "invalid config must not initialize managed storage"
        );
        assert!(
            !env.slate_cache_dir().exists(),
            "invalid config must not initialize cache"
        );
        assert!(!error.contains("PRIVATE_CONTENT"));
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
