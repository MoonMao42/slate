use crate::env::SlateEnv;
use crate::error::Result;
use std::fs;
use std::path::{Path, PathBuf};

mod auto_theme;
mod backup;
mod flags;
mod integration;
mod preferences;
pub(crate) mod shell_integration;
mod state_files;
mod tracked_state;

pub use backup::{
    backup_directory, backup_directory_with_env, begin_restore_point_baseline,
    begin_restore_point_baseline_with_env, clear_all_restore_points, create_backup_with_session,
    create_pre_restore_snapshot, create_pre_restore_snapshot_with_env, delete_restore_point,
    display_tools, execute_restore, execute_restore_with_env, get_restore_point,
    get_restore_point_with_env, is_baseline_restore_point, list_restore_points,
    list_restore_points_with_env, snapshot_current_state, snapshot_current_state_with_env,
    BackupSession, OriginalFileState, RestoreEntry, RestoreFileResult, RestorePoint,
    RestoreReceipt,
};

/// Three-tier configuration manager.
/// Manages three tiers per /// 1. Managed tier: ~/.config/slate/managed/{tool}/ — Slate writes here (regenerates freely)
/// 2. Integration tier: ~/.config/{tool}/config — User's entry file (slate ensures it includes managed, never modifies content)
/// 3. User tier: ~/.config/slate/user/{tool}/ — User's custom overrides (slate never touches)
/// Auto-configuration structure for reading/writing auto.toml.
#[derive(Debug, Clone)]
pub struct AutoConfig {
    pub dark_theme: Option<String>,
    pub light_theme: Option<String>,
}

pub struct ConfigManager {
    base_path: PathBuf,   // ~/.config/slate
    backup_root: PathBuf, // ~/.cache/slate/backups
    home_path: PathBuf,
}

impl ConfigManager {
    fn xdg_config_root(&self) -> &Path {
        self.base_path.parent().unwrap_or(self.base_path.as_path())
    }

    /// Create ConfigManager with injected SlateEnv.
    /// All path resolution goes through SlateEnv for testability.
    /// Prefer this method over new() for new code.
    pub fn with_env(env: &SlateEnv) -> Result<Self> {
        let base_path = env.config_dir().to_path_buf();
        let backup_root = env.slate_cache_dir().join("backups");
        let home_path = env.home().to_path_buf();

        fs::create_dir_all(&base_path)?;
        fs::create_dir_all(&backup_root)?;

        Ok(Self {
            base_path,
            backup_root,
            home_path,
        })
    }

    /// Create ConfigManager from process environment (backward compatibility).
    /// Reads $HOME and $XDG_CONFIG_HOME via SlateEnv::from_process().
    /// For new code: use with_env() instead to enable testing with injected paths.
    pub fn new() -> Result<Self> {
        let env = SlateEnv::from_process()?;
        Self::with_env(&env)
    }

    /// Path to managed directory for a tool.
    /// Example: ~/.config/slate/managed/ghostty
    pub fn managed_dir(&self, tool: &str) -> PathBuf {
        self.base_path.join("managed").join(tool)
    }

    /// Path to user override directory for a tool.
    /// Example: ~/.config/slate/user/ghostty
    #[allow(dead_code)]
    fn user_dir(&self, tool: &str) -> PathBuf {
        self.base_path.join("user").join(tool)
    }

    /// Path to backup directory for a tool.
    /// Example: ~/.cache/slate/backups/starship
    #[cfg(test)]
    fn backups_dir(&self, tool: &str) -> PathBuf {
        self.backup_root.join(tool)
    }

    fn current_theme_path(&self) -> PathBuf {
        self.base_path.join("current")
    }

    fn current_font_path(&self) -> PathBuf {
        self.base_path.join("current-font")
    }

    fn current_opacity_path(&self) -> PathBuf {
        self.base_path.join("current-opacity")
    }

    /// Write managed config for a tool.
    /// Slate owns this tier — regenerate freely without losing user data.
    pub fn write_managed_file(&self, tool: &str, filename: &str, content: &str) -> Result<()> {
        state_files::write_managed_file(&self.managed_dir(tool), filename, content)
    }

    /// A simple pre-edit backup is required by.
    pub fn backup_file(&self, config_path: &Path) -> Result<PathBuf> {
        backup::backup_file(&self.backup_root, config_path)
    }

    pub fn edit_config_field(&self, config_path: &Path, keys: &[&str], value: &str) -> Result<()> {
        if !config_path.exists() {
            return Err(crate::error::SlateError::ConfigNotFound(
                config_path.to_string_lossy().to_string(),
            ));
        }

        self.backup_file(config_path)?;

        let content = fs::read_to_string(config_path)?;
        let mut doc = flags::parse_toml_document(&content)?;

        if keys.len() == 1 {
            doc[keys[0]] = toml_edit::value(value);
        } else {
            return Err(crate::error::SlateError::Internal(
                "Multi-level TOML editing not yet supported; use adapter-specific logic"
                    .to_string(),
            ));
        }

        flags::write_document(config_path, &doc)
    }

    /// Get the base path for three-tier config
    pub fn base_path(&self) -> &Path {
        &self.base_path
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::SlateError;
    use tempfile::TempDir;

    fn test_config_manager(base_path: &Path) -> ConfigManager {
        ConfigManager {
            base_path: base_path.to_path_buf(),
            backup_root: base_path.join(".cache/slate/backups"),
            home_path: base_path.to_path_buf(),
        }
    }

    #[test]
    fn test_config_manager_with_env() {
        let temp = TempDir::new().unwrap();
        let env = SlateEnv::with_home(temp.path().to_path_buf());
        let cm = ConfigManager::with_env(&env).unwrap();
        assert!(cm.base_path.ends_with(".config/slate"));
    }

    #[test]
    fn test_managed_file_write() {
        let temp = TempDir::new().unwrap();
        let config_manager = test_config_manager(temp.path());

        let result = config_manager.write_managed_file(
            "ghostty",
            "colors.conf",
            "# Managed config\ncolor0 = #000000",
        );

        assert!(result.is_ok());

        let managed_file = config_manager.managed_dir("ghostty").join("colors.conf");
        assert!(managed_file.exists());

        let content = fs::read_to_string(&managed_file).unwrap();
        assert!(content.contains("color0"));
    }

    #[test]
    fn test_user_dir_path() {
        let temp = TempDir::new().unwrap();
        let config_manager = test_config_manager(temp.path());

        assert_eq!(
            config_manager.user_dir("ghostty"),
            temp.path().join("user").join("ghostty")
        );
    }

    #[test]
    fn test_current_theme_tracking() {
        let temp = TempDir::new().unwrap();
        let config_manager = test_config_manager(temp.path());

        let current = config_manager.get_current_theme().unwrap();
        assert_eq!(current, None);

        config_manager
            .set_current_theme("catppuccin-mocha")
            .unwrap();
        let current = config_manager.get_current_theme().unwrap();
        assert_eq!(current, Some("catppuccin-mocha".to_string()));
    }

    #[test]
    fn test_shell_integration_file_guards_optional_zsh_source() {
        let temp = TempDir::new().unwrap();
        let config_manager = test_config_manager(temp.path());
        let theme = crate::theme::catppuccin::catppuccin_mocha().unwrap();

        config_manager.write_shell_integration_file(&theme).unwrap();

        let shell_file = config_manager.managed_dir("shell").join("env.zsh");
        let content = fs::read_to_string(shell_file).unwrap();
        let expected = crate::detection::shell_quote(
            &config_manager
                .base_path()
                .join("managed/zsh/highlight-styles.sh")
                .to_string_lossy(),
        );
        assert!(content.contains(&format!("if [ -f {} ]; then", expected)));
        assert!(content.contains(&format!("source {}", expected)));
    }

    #[test]
    fn test_shell_integration_file_uses_injected_managed_and_xdg_paths() {
        let temp = TempDir::new().unwrap();
        let base_path = temp.path().join("custom-xdg/slate");
        let config_manager = test_config_manager(&base_path);
        let theme = crate::theme::catppuccin::catppuccin_mocha().unwrap();

        config_manager.write_shell_integration_file(&theme).unwrap();

        let shell_file = config_manager.managed_dir("shell").join("env.zsh");
        let content = fs::read_to_string(shell_file).unwrap();

        let managed_root = base_path.join("managed").to_string_lossy().to_string();
        let xdg_root = base_path.parent().unwrap().to_string_lossy().to_string();
        let eza_config = crate::detection::shell_quote(&format!("{}/eza", managed_root));
        let lazygit_config = crate::detection::shell_quote(&format!(
            "{}/lazygit/config.yml:{}/lazygit/config.yml",
            managed_root, xdg_root
        ));
        let active_starship = crate::detection::shell_quote(
            &base_path
                .parent()
                .unwrap()
                .join("starship.toml")
                .to_string_lossy(),
        );
        let plain_starship = crate::detection::shell_quote(
            &base_path
                .join("managed")
                .join("starship")
                .join("plain.toml")
                .to_string_lossy(),
        );

        assert!(content.contains(&format!("export EZA_CONFIG_DIR={}", eza_config)));
        assert!(content.contains(&format!("export LG_CONFIG_FILE={}", lazygit_config)));
        assert!(content.contains(&format!("export STARSHIP_CONFIG={}", active_starship)));
        assert!(content.contains(&plain_starship));
        assert!(!content.contains("$HOME/.config/slate"));
    }

    #[test]
    fn test_shell_integration_uses_single_watcher_guard() {
        let temp = TempDir::new().unwrap();
        let config_manager = test_config_manager(temp.path());
        let theme = crate::theme::catppuccin::catppuccin_mocha().unwrap();

        config_manager.set_auto_theme_enabled(true).unwrap();
        config_manager.write_shell_integration_file(&theme).unwrap();

        let shell_file = config_manager.managed_dir("shell").join("env.zsh");
        let content = fs::read_to_string(shell_file).unwrap();
        assert!(content.contains("if ! pgrep -f \"slate-dark-mode-notify\" >/dev/null 2>&1; then"));
        assert!(content.contains("theme --auto --quiet >/dev/null 2>&1 &"));
        assert!(!content.contains("_SLATE_AUTO_WATCHER"));
    }

    #[test]
    fn test_auto_config_uses_toml_parser_and_writer() {
        let temp = TempDir::new().unwrap();
        let config_manager = test_config_manager(temp.path());
        let auto_path = temp.path().join("auto.toml");

        fs::write(
            &auto_path,
            "# comment\ndark_theme = \"catppuccin-mocha\"\nlight_theme = \"catppuccin-latte\"\n",
        )
        .unwrap();

        let auto_config = config_manager.read_auto_config().unwrap().unwrap();
        assert_eq!(auto_config.dark_theme.as_deref(), Some("catppuccin-mocha"));
        assert_eq!(auto_config.light_theme.as_deref(), Some("catppuccin-latte"));

        let injected = "theme\"\nlight_theme = \"pwned";
        config_manager
            .write_auto_config(Some(injected), Some("catppuccin-latte"))
            .unwrap();

        let round_trip = config_manager.read_auto_config().unwrap().unwrap();
        assert_eq!(round_trip.dark_theme.as_deref(), Some(injected));
        assert_eq!(round_trip.light_theme.as_deref(), Some("catppuccin-latte"));
    }

    #[test]
    fn test_edit_config_field_single_level() {
        let temp = TempDir::new().unwrap();
        let config_path = temp.path().join("config.toml");

        let initial = r#"
palette = "old"
format = "..."
"#;
        fs::write(&config_path, initial).unwrap();

        let config_manager = test_config_manager(temp.path());
        let result = config_manager.edit_config_field(&config_path, &["palette"], "new-palette");

        assert!(result.is_ok());

        let content = fs::read_to_string(&config_path).unwrap();
        assert!(content.contains("new-palette"));
        assert!(!content.contains("\"old\""));

        let backup_dir = config_manager.backups_dir("config");
        let backup_files: Vec<_> = fs::read_dir(&backup_dir).unwrap().collect();
        assert_eq!(backup_files.len(), 1);
    }

    #[test]
    fn test_backup_file_uses_inferred_tool_name() {
        let temp = TempDir::new().unwrap();
        let config_path = temp.path().join("starship.toml");
        fs::write(&config_path, "palette = \"catppuccin-mocha\"\n").unwrap();

        let config_manager = test_config_manager(&temp.path().join(".config/slate"));
        fs::create_dir_all(config_manager.base_path()).unwrap();

        let backup_path = config_manager.backup_file(&config_path).unwrap();

        assert!(backup_path.starts_with(config_manager.backups_dir("starship")));
        assert!(backup_path.exists());
    }

    #[test]
    fn test_opacity_persistence_missing_file_defaults_to_solid() {
        let temp = TempDir::new().unwrap();
        let config_manager = test_config_manager(temp.path());
        let preset = config_manager.get_current_opacity_preset().unwrap();
        assert_eq!(preset, crate::opacity::OpacityPreset::Solid);
    }

    #[test]
    fn test_opacity_persistence_round_trip() {
        let temp = TempDir::new().unwrap();
        let config_manager = test_config_manager(temp.path());

        config_manager
            .set_current_opacity_preset(crate::opacity::OpacityPreset::Frosted)
            .unwrap();

        let preset = config_manager.get_current_opacity_preset().unwrap();
        assert_eq!(preset, crate::opacity::OpacityPreset::Frosted);

        let path = config_manager.current_opacity_path();
        let content = fs::read_to_string(&path).unwrap();
        assert_eq!(content, "frosted");
    }

    #[test]
    fn test_opacity_persistence_all_presets() {
        let temp = TempDir::new().unwrap();
        let config_manager = test_config_manager(temp.path());

        for preset in &[
            crate::opacity::OpacityPreset::Solid,
            crate::opacity::OpacityPreset::Frosted,
            crate::opacity::OpacityPreset::Clear,
        ] {
            config_manager.set_current_opacity_preset(*preset).unwrap();
            let read_preset = config_manager.get_current_opacity_preset().unwrap();
            assert_eq!(&read_preset, preset);
        }
    }

    #[test]
    fn test_fastfetch_autorun_marker_toggle() {
        let temp = TempDir::new().unwrap();
        let config_manager = test_config_manager(temp.path());

        let enabled = config_manager.has_fastfetch_autorun().unwrap();
        assert!(!enabled);

        config_manager.enable_fastfetch_autorun().unwrap();
        let enabled = config_manager.has_fastfetch_autorun().unwrap();
        assert!(enabled);

        let path = config_manager.base_path().join("autorun-fastfetch");
        assert!(path.exists());

        config_manager.disable_fastfetch_autorun().unwrap();
        let enabled = config_manager.has_fastfetch_autorun().unwrap();
        assert!(!enabled);
    }

    #[test]
    fn test_fastfetch_disable_is_idempotent() {
        let temp = TempDir::new().unwrap();
        let config_manager = test_config_manager(temp.path());

        let result = config_manager.disable_fastfetch_autorun();
        assert!(result.is_ok());

        let result = config_manager.disable_fastfetch_autorun();
        assert!(result.is_ok());
    }

    #[test]
    fn test_fastfetch_disable_propagates_non_not_found_errors() {
        let temp = TempDir::new().unwrap();
        let config_manager = test_config_manager(temp.path());
        let marker_path = temp.path().join("autorun-fastfetch");
        fs::create_dir(&marker_path).unwrap();

        let result = config_manager.disable_fastfetch_autorun();
        assert!(matches!(result, Err(SlateError::IOError(_))));
    }

    #[test]
    fn test_write_shell_integration_includes_fastfetch_when_marker_present() {
        let temp = TempDir::new().unwrap();
        let config_manager = test_config_manager(temp.path());

        config_manager.enable_fastfetch_autorun().unwrap();

        let registry = crate::theme::ThemeRegistry::new().unwrap();
        let theme = registry.get("catppuccin-mocha").unwrap();

        config_manager.write_shell_integration_file(theme).unwrap();

        let env_zsh_path = temp.path().join("managed/shell/env.zsh");
        let content = std::fs::read_to_string(&env_zsh_path).unwrap();

        assert!(content.contains("if command -v fastfetch >/dev/null 2>&1; then"));
        assert!(content.contains("  fastfetch"));
        assert!(content.contains("fi"));
        assert!(temp.path().join("managed/shell/env.bash").exists());
        assert!(temp.path().join("managed/shell/env.fish").exists());
    }

    #[test]
    fn test_write_shell_integration_excludes_fastfetch_when_marker_absent() {
        let temp = TempDir::new().unwrap();
        let config_manager = test_config_manager(temp.path());

        let registry = crate::theme::ThemeRegistry::new().unwrap();
        let theme = registry.get("catppuccin-mocha").unwrap();

        config_manager.write_shell_integration_file(theme).unwrap();

        let env_zsh_path = temp.path().join("managed/shell/env.zsh");
        let content = std::fs::read_to_string(&env_zsh_path).unwrap();

        assert!(!content.contains("if command -v fastfetch >/dev/null 2>&1; then"));
        assert!(!content.contains("  fastfetch\nfi"));
    }

    #[test]
    fn test_refresh_shell_integration_uses_current_theme() {
        let temp = TempDir::new().unwrap();
        let config_manager = test_config_manager(temp.path());

        config_manager
            .set_current_theme("tokyo-night-dark")
            .unwrap();
        config_manager.refresh_shell_integration().unwrap();

        let env_zsh_path = temp.path().join("managed/shell/env.zsh");
        let content = std::fs::read_to_string(&env_zsh_path).unwrap();

        assert!(content.contains("export BAT_THEME='Tokyo Night'"));
        assert!(temp.path().join("managed/shell/env.bash").exists());
        assert!(temp.path().join("managed/shell/env.fish").exists());
    }

    #[test]
    fn test_refresh_shell_integration_falls_back_to_default_theme() {
        let temp = TempDir::new().unwrap();
        let config_manager = test_config_manager(temp.path());

        config_manager.set_current_theme("missing-theme").unwrap();
        config_manager.refresh_shell_integration().unwrap();

        let env_zsh_path = temp.path().join("managed/shell/env.zsh");
        let content = std::fs::read_to_string(&env_zsh_path).unwrap();

        assert!(content.contains("export BAT_THEME='Catppuccin Mocha'"));
    }

    #[test]
    fn test_write_shell_integration_writes_all_shell_files() {
        let temp = TempDir::new().unwrap();
        let config_manager = test_config_manager(temp.path());
        let theme = crate::theme::catppuccin::catppuccin_mocha().unwrap();

        config_manager.write_shell_integration_file(&theme).unwrap();

        assert!(temp.path().join("managed/shell/env.zsh").exists());
        assert!(temp.path().join("managed/shell/env.bash").exists());
        assert!(temp.path().join("managed/shell/env.fish").exists());
    }

    #[test]
    fn test_refresh_shell_integration_writes_bash_and_fish_content() {
        let temp = TempDir::new().unwrap();
        let config_manager = test_config_manager(temp.path());

        config_manager
            .set_current_theme("catppuccin-mocha")
            .unwrap();
        config_manager.refresh_shell_integration().unwrap();

        let env_bash = std::fs::read_to_string(temp.path().join("managed/shell/env.bash")).unwrap();
        let env_fish = std::fs::read_to_string(temp.path().join("managed/shell/env.fish")).unwrap();

        assert!(env_bash.contains("starship init bash"));
        assert!(env_fish.contains("starship init fish | source"));
        assert!(env_fish.contains("set -gx BAT_THEME "));
    }

    #[test]
    fn test_shell_integration_prefers_plain_starship_for_system_font() {
        let temp = TempDir::new().unwrap();
        let config_manager = test_config_manager(temp.path());
        let theme = crate::theme::catppuccin::catppuccin_mocha().unwrap();

        config_manager.set_starship_enabled(true).unwrap();
        config_manager.set_current_font("Menlo").unwrap();
        config_manager.write_shell_integration_file(&theme).unwrap();

        let env_zsh_path = temp.path().join("managed/shell/env.zsh");
        let content = std::fs::read_to_string(&env_zsh_path).unwrap();

        assert!(content.contains("export STARSHIP_CONFIG="));
        assert!(content.contains("/managed/starship/plain.toml"));
        assert!(!content.contains("else\n  export STARSHIP_CONFIG="));
    }

    #[test]
    fn test_shell_integration_keeps_active_starship_for_nerd_font() {
        let temp = TempDir::new().unwrap();
        let config_manager = test_config_manager(temp.path());
        let theme = crate::theme::catppuccin::catppuccin_mocha().unwrap();

        config_manager.set_starship_enabled(true).unwrap();
        config_manager
            .set_current_font("JetBrainsMono Nerd Font")
            .unwrap();
        config_manager.write_shell_integration_file(&theme).unwrap();

        let env_zsh_path = temp.path().join("managed/shell/env.zsh");
        let content = std::fs::read_to_string(&env_zsh_path).unwrap();

        assert!(content.contains("if [ -f '"));
        assert!(content.contains("export STARSHIP_CONFIG='"));
        assert!(content.contains("plain.toml"));
    }
}
