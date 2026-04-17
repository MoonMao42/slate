use super::ConfigManager;
use crate::env::SlateEnv;
use crate::error::{Result, SlateError};

impl ConfigManager {
    /// Write managed shell integration files with theme-aware content.
    /// Called both during setup (to initialize) and on theme/config refresh.
    pub fn write_shell_integration_file(&self, theme: &crate::theme::ThemeVariant) -> Result<()> {
        let managed_root = self.base_path.join("managed");
        let managed_root = managed_root.to_string_lossy().to_string();
        let user_config_root = self.xdg_config_root().to_string_lossy().to_string();
        let user_local_bin = self
            .home_path
            .join(".local/bin")
            .to_string_lossy()
            .to_string();
        let plain_starship_path = self
            .managed_dir("starship")
            .join("plain.toml")
            .to_string_lossy()
            .to_string();
        let active_starship_path = self
            .xdg_config_root()
            .join("starship.toml")
            .to_string_lossy()
            .to_string();
        let notify_path = self
            .managed_dir("bin")
            .join("slate-dark-mode-notify")
            .to_string_lossy()
            .to_string();
        let zsh_highlighting_plugin_path =
            crate::detection::detect_zsh_syntax_highlighting_plugin(&self.home_path)
                .map(|path| path.to_string_lossy().to_string());
        let slate_bin = std::env::current_exe()
            .ok()
            .map(|path| path.to_string_lossy().to_string())
            .unwrap_or_else(|| "slate".to_string());
        let prefer_plain_starship = self.should_prefer_plain_starship()?;
        let starship_enabled = self.is_starship_enabled()?;

        let contents = super::shell_integration::build_shell_integration_files(
            theme,
            &super::shell_integration::ShellIntegrationOptions {
                managed_root: &managed_root,
                user_config_root: &user_config_root,
                user_local_bin: Some(&user_local_bin),
                plain_starship_path: &plain_starship_path,
                active_starship_path: &active_starship_path,
                notify_path: &notify_path,
                slate_bin: &slate_bin,
                zsh_highlighting_plugin_path: zsh_highlighting_plugin_path.as_deref(),
                homebrew_prefix: crate::detection::homebrew_prefix()
                    .as_ref()
                    .and_then(|path| path.to_str()),
                prefer_plain_starship,
                starship_enabled,
                zsh_highlighting_enabled: self.is_zsh_highlighting_enabled()?,
                fastfetch_autorun: self.has_fastfetch_autorun()?,
                auto_theme_enabled: self.is_auto_theme_enabled()?,
            },
        );

        self.write_managed_file(
            "starship",
            "plain.toml",
            &super::shell_integration::themed_plain_starship_content(theme),
        )?;
        self.write_managed_file("shell", "env.zsh", &contents.zsh)?;
        self.write_managed_file("shell", "env.bash", &contents.bash)?;
        self.write_managed_file("shell", "env.fish", &contents.fish)?;
        Ok(())
    }

    fn should_prefer_plain_starship(&self) -> Result<bool> {
        self.has_no_nerd_font()
    }

    /// Returns true if no Nerd Font is configured or detected on the system.
    fn has_no_nerd_font(&self) -> Result<bool> {
        let env = SlateEnv::with_home(self.home_path.clone());
        if let Some(font) = self.get_current_font()? {
            return Ok(!crate::adapter::font::FontAdapter::is_nerd_font_name(&font));
        }

        let installed =
            crate::adapter::font::FontAdapter::detect_installed_nerd_fonts_with_env(&env)?;
        Ok(installed.is_empty())
    }

    /// Rebuild shell integration using the current theme tracking file.
    /// Falls back to the default theme when no current theme is recorded or
    /// when the tracked theme ID no longer exists in the registry.
    pub fn refresh_shell_integration(&self) -> Result<()> {
        let registry = crate::theme::ThemeRegistry::new()?;
        let tracked_theme_id = self
            .get_current_theme()?
            .unwrap_or_else(|| crate::theme::DEFAULT_THEME_ID.to_string());

        let theme = registry
            .get(&tracked_theme_id)
            .or_else(|| registry.get(crate::theme::DEFAULT_THEME_ID))
            .cloned()
            .ok_or_else(|| {
                SlateError::InvalidThemeData(format!(
                    "Unable to resolve shell integration theme from tracked id '{}'",
                    tracked_theme_id
                ))
            })?;

        self.write_shell_integration_file(&theme)
    }
}
