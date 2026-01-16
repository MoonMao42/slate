use crate::env::SlateEnv;
use crate::error::{Result, SlateError};
use atomic_write_file::AtomicWriteFile;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use toml_edit::DocumentMut;

mod backup;
pub use backup::{
    begin_restore_point_baseline, begin_restore_point_baseline_with_env,
    is_baseline_restore_point, list_restore_points, list_restore_points_with_env, BackupInfo,
    BackupSession, OriginalFileState, RestoreEntry, RestorePoint,
};

/// Three-tier configuration manager.
/// Manages three tiers per /// 1. Managed tier: ~/.config/slate/managed/{tool}/ — Slate writes here (regenerates freely)
/// 2. Integration tier: ~/.config/{tool}/config — User's entry file (slate ensures it includes managed, never modifies content)
/// 3. User tier: ~/.config/slate/user/{tool}/ — User's custom overrides (slate never touches)
/// Auto-configuration structure for reading/writing auto.toml
#[derive(Debug, Clone)]
pub struct AutoConfig {
    pub dark_theme: Option<String>,
    pub light_theme: Option<String>,
}

pub struct ConfigManager {
    base_path: PathBuf, // ~/.config/slate
    backup_root: PathBuf, // ~/.cache/slate/backups
}

impl ConfigManager {
    fn xdg_config_root(&self) -> &Path {
        self.base_path.parent().unwrap_or(self.base_path.as_path())
    }

    fn parse_toml_document(content: &str) -> Result<DocumentMut> {
        if content.trim().is_empty() {
            Ok(DocumentMut::new())
        } else {
            Ok(content.parse::<DocumentMut>()?)
        }
    }

    fn read_auto_theme_value(doc: &DocumentMut, key: &str) -> Result<Option<String>> {
        match doc.get(key) {
            Some(item) => item
                .as_str()
                .map(|value| value.to_string())
                .map(Some)
                .ok_or_else(|| {
                    SlateError::InvalidConfig(format!("auto.toml field '{}' must be a string", key))
                }),
            None => Ok(None),
        }
    }

    /// Create ConfigManager with injected SlateEnv.
    /// All path resolution goes through SlateEnv for testability.
    /// Prefer this method over new() for new code.
    pub fn with_env(env: &SlateEnv) -> Result<Self> {
        let base_path = env.config_dir().to_path_buf();
        let backup_root = env.slate_cache_dir().join("backups");

        // Ensure base directory exists
        fs::create_dir_all(&base_path)?;
        fs::create_dir_all(&backup_root)?;

        Ok(Self {
            base_path,
            backup_root,
        })
    }

    /// Create ConfigManager from process environment (backward compatibility).
    /// Reads $HOME and $XDG_CONFIG_HOME via SlateEnv::from_process().
    /// For new code: use with_env() instead to enable testing with injected paths.
    pub fn new() -> Result<Self> {
        let env = SlateEnv::from_process()?;
        Self::with_env(&env)
    }

    /// Path to managed directory for a tool
    /// Example: ~/.config/slate/managed/ghostty
    pub fn managed_dir(&self, tool: &str) -> PathBuf {
        self.base_path.join("managed").join(tool)
    }

    /// Path to user override directory for a tool
    /// Example: ~/.config/slate/user/ghostty
    #[allow(dead_code)]
    fn user_dir(&self, tool: &str) -> PathBuf {
        self.base_path.join("user").join(tool)
    }

    /// Path to backup directory for a tool
    /// Example: ~/.cache/slate/backups/starship
    fn backups_dir(&self, tool: &str) -> PathBuf {
        self.backup_root.join(tool)
    }

    /// Path to current theme tracking file
    /// ~/.config/slate/current — plain text file with theme ID 
    fn current_theme_path(&self) -> PathBuf {
        self.base_path.join("current")
    }

    /// Path to selected font tracking file
    /// ~/.config/slate/current-font — plain text with canonical font family name
    fn current_font_path(&self) -> PathBuf {
        self.base_path.join("current-font")
    }

    /// Write managed config for a tool.
    /// Slate owns this tier — regenerate freely without losing user data.
    /// Use atomic_write_file to prevent partial writes.
    /// Per RESEARCH: canonicalize the managed directory and reject symlink targets.
    pub fn write_managed_file(&self, tool: &str, filename: &str, content: &str) -> Result<()> {
        let dir = self.managed_dir(tool);
        fs::create_dir_all(&dir)?;

        let canonical_dir = fs::canonicalize(&dir)?;
        let path = canonical_dir.join(filename);

        if path.exists() && fs::symlink_metadata(&path)?.file_type().is_symlink() {
            return Err(SlateError::InvalidConfig(format!(
                "Refusing to write managed config through symlink: {}",
                path.display()
            )));
        }

        // Atomic write to prevent TOCTOU and partial writes
        let mut file = AtomicWriteFile::open(&path)?;
        file.write_all(content.as_bytes())?;
        file.commit()?;

        Ok(())
    }
    /// Write shell integration file (env.zsh) with theme-aware content.
    /// Per , generates exports + fastfetch wrapper + zsh-highlight source.
    /// Called both during setup (to initialize) and on `slate set` (to update).
    pub fn write_shell_integration_file(&self, theme: &crate::theme::ThemeVariant) -> Result<()> {
        let mut content = String::new();
        let managed_root = self.base_path.join("managed");
        let managed_root = managed_root.to_string_lossy();
        let user_config_root = self.xdg_config_root().to_string_lossy();
        let plain_starship_path = self.managed_dir("starship").join("plain.toml");
        let plain_starship_path = plain_starship_path.to_string_lossy();
        let notify_bin = self.managed_dir("bin").join("slate-dark-mode-notify");
        let notify_path = notify_bin.to_string_lossy();
        let slate_bin = std::env::current_exe()
            .ok()
            .map(|path| format!("\"{}\"", path.to_string_lossy()))
            .unwrap_or_else(|| "slate".to_string());

        // Export BAT_THEME
        content.push_str(&format!(
            "export BAT_THEME=\"{}\"
",
            theme
                .tool_refs
                .get("bat")
                .map(|s| s.as_str())
                .unwrap_or("Catppuccin Mocha")
        ));

        // Export EZA_CONFIG_DIR
        content.push_str(&format!(
            "export EZA_CONFIG_DIR=\"{}/eza\"\n",
            managed_root
        ));

        // Export LG_CONFIG_FILE
        content.push_str(&format!(
            "export LG_CONFIG_FILE=\"{managed}/lazygit/config.yml:{xdg}/lazygit/config.yml\"\n",
            managed = managed_root,
            xdg = user_config_root
        ));

        // Add fastfetch wrapper function
        content.push_str(&format!(
            "fastfetch() {{ command fastfetch -c \"{}/fastfetch/config.jsonc\" \"$@\"; }}\n",
            managed_root
        ));

        // Guard optional zsh-syntax-highlighting styles so fresh shells do not
        // fail when the plugin has not been installed yet.
        content.push_str(&format!(
            "if [ -f \"{managed}/zsh/highlight-styles.sh\" ]; then\n  source \"{managed}/zsh/highlight-styles.sh\"\nfi\n",
            managed = managed_root
        ));
        // Gate features: only Terminal.app needs a plain starship (no Nerd Font glyphs).
        // All modern terminals (Ghostty, Alacritty, iTerm2, WezTerm, Kitty, etc.)
        // render Nerd Fonts fine, so we use an exclusion list instead of an allow list.
        // TERM_PROGRAM comparison is case-insensitive to handle Ghostty variants.
        content.push_str("\nif [[ \"$TERM_PROGRAM\" != \"Apple_Terminal\" ]]; then\n");

        // Conditionally run fastfetch on terminal open if auto-run enabled
        if self.has_fastfetch_autorun()? {
            content.push_str("  if command -v fastfetch &> /dev/null; then\n");
            content.push_str("    fastfetch\n");
            content.push_str("  fi\n");
        }

        // Auto-theme watcher: Ghostty-only. Other terminals don't need it,
        // and the Swift NSApplication binary triggers macOS permission prompts
        // (automation/accessibility) in non-Ghostty terminals.
        if self.is_auto_theme_enabled()? {
            content.push_str(&format!(
                r#"  if [[ "${{TERM_PROGRAM:l}}" == "ghostty" ]] && [[ -x "{path}" ]]; then
    if ! pgrep -qf "slate-dark-mode-notify"; then
      "{path}" {slate_bin} theme --auto --quiet &!
    fi
  fi
"#,
                path = notify_path,
                slate_bin = slate_bin
            ));
        }

        content.push_str("else\n");
        // Non-Ghostty terminals: use a plain starship config without Nerd Font glyphs
        content.push_str(&format!(
            "  export STARSHIP_CONFIG=\"{}\"\n",
            plain_starship_path
        ));
        content.push_str("fi\n");

        // Write plain starship config for non-Ghostty terminals
        let plain_content = r#"format = "$username$directory$git_branch$git_status$cmd_duration$line_break$character"

[username]
show_always = true
format = "[$user]($style) "
style_user = "bold green"

[directory]
format = "[$path]($style) "
style = "bold cyan"
truncation_length = 3

[git_branch]
format = "[$symbol$branch]($style) "
symbol = ""
style = "bold purple"

[git_status]
format = "([$all_status$ahead_behind]($style) )"
style = "bold red"

[cmd_duration]
format = "[$duration]($style) "
style = "bold yellow"

[character]
success_symbol = "[>](bold green)"
error_symbol = "[>](bold red)"
"#;
        self.write_managed_file("starship", "plain.toml", plain_content)?;

        // Write atomically to ~/.config/slate/managed/shell/env.zsh
        self.write_managed_file("shell", "env.zsh", &content)?;

        Ok(())
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

    /// Update current theme tracking file.
    /// plain text file with theme ID.
    pub fn set_current_theme(&self, theme_id: &str) -> Result<()> {
        let path = self.current_theme_path();
        let mut file = AtomicWriteFile::open(&path)?;
        file.write_all(theme_id.as_bytes())?;
        file.commit()?;
        Ok(())
    }

    /// Get current theme ID from tracking file.
    pub fn get_current_theme(&self) -> Result<Option<String>> {
        let path = self.current_theme_path();

        if !path.exists() {
            return Ok(None);
        }

        let content = fs::read_to_string(&path)?;
        Ok(Some(content.trim().to_string()))
    }

    /// Persist user's chosen font family name.
    pub fn set_current_font(&self, font_family: &str) -> Result<()> {
        let path = self.current_font_path();
        let mut file = AtomicWriteFile::open(&path)?;
        file.write_all(font_family.as_bytes())?;
        file.commit()?;
        Ok(())
    }

    /// Get the user's chosen font family name.
    pub fn get_current_font(&self) -> Result<Option<String>> {
        let path = self.current_font_path();
        if !path.exists() {
            return Ok(None);
        }
        let content = fs::read_to_string(&path)?;
        let trimmed = content.trim();
        if trimmed.is_empty() {
            return Ok(None);
        }
        Ok(Some(trimmed.to_string()))
    }

    /// Path to current opacity tracking file
    /// ~/.config/slate/current-opacity — plain text with opacity preset (solid|frosted|clear)
    /// Note: This is temporary for 06-01 Hub. 06-03 will add full opacity persistence.
    fn current_opacity_path(&self) -> PathBuf {
        self.base_path.join("current-opacity")
    }

    /// Get the current opacity preset.
    /// Note: This is temporary for 06-01 Hub. 06-03 will enhance this implementation.
    pub fn get_current_opacity(&self) -> Result<Option<String>> {
        let path = self.current_opacity_path();
        if !path.exists() {
            return Ok(None);
        }
        let content = fs::read_to_string(&path)?;
        let trimmed = content.trim();
        if trimmed.is_empty() {
            return Ok(None);
        }
        Ok(Some(trimmed.to_string()))
    }

    /// Get the current opacity preset, parsing from file.
    /// Returns OpacityPreset::Solid if file missing (fallback).
    /// Per D-08b: Opacity persists independently of theme.
    pub fn get_current_opacity_preset(&self) -> Result<crate::opacity::OpacityPreset> {
        let path = self.current_opacity_path();

        if !path.exists() {
            // Fallback to Solid when file missing
            return Ok(crate::opacity::OpacityPreset::Solid);
        }

        let content = fs::read_to_string(&path)?;
        let trimmed = content.trim();

        if trimmed.is_empty() {
            // Fallback to Solid when file is empty
            return Ok(crate::opacity::OpacityPreset::Solid);
        }

        // Parse opacity preset from file (FromStr trait is auto in scope)
        trimmed.parse::<crate::opacity::OpacityPreset>()
    }

    /// Set the current opacity preset, persisting to file.
    /// Opacity persists independently of theme.
    /// Called on explicit commit/apply paths: Enter, explicit `slate set`, `--auto`, setup wizard completion.
    /// Atomic write pattern: temp file + rename to prevent TOCTOU.
    pub fn set_current_opacity_preset(&self, preset: crate::opacity::OpacityPreset) -> Result<()> {
        let path = self.current_opacity_path();

        // Write as lowercase string (no newline)
        let content = preset.to_string().to_lowercase();

        let mut file = AtomicWriteFile::open(&path)?;
        file.write_all(content.as_bytes())?;
        file.commit()?;

        Ok(())
    }

    /// Read auto.toml from ~/.config/slate/auto.toml if it exists.
    /// Returns None if file doesn't exist; error if file is unreadable.
    pub fn read_auto_config(&self) -> Result<Option<AutoConfig>> {
        let path = self.base_path.join("auto.toml");

        if !path.exists() {
            return Ok(None);
        }

        let content = fs::read_to_string(&path)?;
        let doc = Self::parse_toml_document(&content)?;
        let dark_theme = Self::read_auto_theme_value(&doc, "dark_theme")?;
        let light_theme = Self::read_auto_theme_value(&doc, "light_theme")?;

        Ok(Some(AutoConfig {
            dark_theme,
            light_theme,
        }))
    }

    /// Write auto.toml with the specified dark and light themes.
    /// File is flat TOML with two optional string fields.
    /// Preserves existing field if new value is None.
    /// Uses atomic write to prevent corruption.
    pub fn write_auto_config(
        &self,
        dark_theme: Option<&str>,
        light_theme: Option<&str>,
    ) -> Result<()> {
        let current = self.read_auto_config()?;
        let path = self.base_path.join("auto.toml");
        let mut doc = if path.exists() {
            Self::parse_toml_document(&fs::read_to_string(&path)?)?
        } else {
            DocumentMut::new()
        };

        // Merge with new values (new values take precedence)
        let final_dark = dark_theme
            .map(String::from)
            .or(current.as_ref().and_then(|c| c.dark_theme.clone()));
        let final_light = light_theme
            .map(String::from)
            .or(current.as_ref().and_then(|c| c.light_theme.clone()));

        if let Some(dark) = final_dark {
            doc["dark_theme"] = toml_edit::value(dark);
        } else {
            doc.remove("dark_theme");
        }
        if let Some(light) = final_light {
            doc["light_theme"] = toml_edit::value(light);
        } else {
            doc.remove("light_theme");
        }

        // Write atomically
        let mut file = AtomicWriteFile::open(&path)?;
        file.write_all(doc.to_string().as_bytes())?;
        file.commit()?;

        Ok(())
    }

    /// Check if auto-theme is enabled via config.toml [auto_theme].enabled field.
    /// If config.toml or [auto_theme] section missing, defaults to false.
    pub fn is_auto_theme_enabled(&self) -> Result<bool> {
        let config_path = self.base_path.join("config.toml");

        if !config_path.exists() {
            return Ok(false);
        }

        let content = fs::read_to_string(&config_path)?;
        let doc = content.parse::<DocumentMut>()?;

        // Read [auto_theme].enabled; default to false if missing
        let enabled = doc
            .get("auto_theme")
            .and_then(|table| table.get("enabled"))
            .and_then(|value| value.as_bool())
            .unwrap_or(false);

        Ok(enabled)
    }

    /// Write auto-theme enabled flag to config.toml.
    /// Writes to [auto_theme].enabled field using atomic write.
    pub fn set_auto_theme_enabled(&self, enabled: bool) -> Result<()> {
        let config_path = self.base_path.join("config.toml");

        // Read existing config or start with empty
        let mut doc = if config_path.exists() {
            fs::read_to_string(&config_path)?.parse::<DocumentMut>()?
        } else {
            DocumentMut::new()
        };

        // Ensure [auto_theme] table exists
        if !doc.contains_key("auto_theme") {
            doc.insert("auto_theme", toml_edit::table());
        }

        // Set the enabled field
        doc["auto_theme"]["enabled"] = toml_edit::value(enabled);

        // Write atomically
        let mut file = AtomicWriteFile::open(&config_path)?;
        file.write_all(doc.to_string().as_bytes())?;
        file.commit()?;

        Ok(())
    }

    /// Edit a field in a TOML config file using AST-aware editing.
    /// Per RESEARCH Pitfall 1: Use toml_edit, never regex.
    /// Preserves comments and formatting.
    /// Note: This implementation supports single-level keys only.
    /// For nested keys, adapters should implement their own TOML editing logic.
    /// A simple pre-edit backup is required by.
    pub fn backup_file(&self, config_path: &Path) -> Result<PathBuf> {
        if !config_path.exists() {
            return Err(SlateError::ConfigNotFound(
                config_path.to_string_lossy().to_string(),
            ));
        }

        let tool = Self::infer_tool_name(config_path);
        let backup_dir = self.backups_dir(&tool);
        fs::create_dir_all(&backup_dir)?;

        let original_name = config_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("config");
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let backup_path = backup_dir.join(format!("{timestamp}-{original_name}.bak"));

        fs::copy(config_path, &backup_path)?;

        Ok(backup_path)
    }

    /// Example:
    /// ```ignore
    /// config.edit_config_field(
    /// Path::new("~/.config/starship.toml"),
    /// &["palette"],
    /// "catppuccin-mocha"
    /// )?;
    /// ```

    /// Check if fastfetch auto-run is enabled via marker file.
    /// Marker is an empty file; existence = enabled.
    pub fn has_fastfetch_autorun(&self) -> Result<bool> {
        let path = self.base_path.join("autorun-fastfetch");
        Ok(path.exists())
    }

    /// Enable fastfetch auto-run by creating marker file atomically.
    /// Creates empty file at ~/.config/slate/autorun-fastfetch
    pub fn enable_fastfetch_autorun(&self) -> Result<()> {
        let path = self.base_path.join("autorun-fastfetch");
        let mut file = AtomicWriteFile::open(&path)?;
        file.write_all(b"")?;
        file.commit()?;
        Ok(())
    }

    /// Disable fastfetch auto-run by deleting marker file.
    /// Best-effort deletion; no error if file doesn't exist.
    pub fn disable_fastfetch_autorun(&self) -> Result<()> {
        let path = self.base_path.join("autorun-fastfetch");
        match fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(err) => Err(err.into()),
        }
    }
    pub fn edit_config_field(&self, config_path: &Path, keys: &[&str], value: &str) -> Result<()> {
        if !config_path.exists() {
            return Err(crate::error::SlateError::ConfigNotFound(
                config_path.to_string_lossy().to_string(),
            ));
        }

        self.backup_file(config_path)?;

        // Read existing TOML
        let content = fs::read_to_string(config_path)?;
        let mut doc: DocumentMut = content.parse()?;

        // For now, support only top-level keys (single element in keys array)
        // adapters can implement their own more complex TOML editing
        if keys.len() == 1 {
            doc[keys[0]] = toml_edit::value(value);
        } else {
            // For multi-level keys, we'd need recursive navigation which is complex with DocumentMut
            // Adapters should implement custom logic instead
            return Err(crate::error::SlateError::Internal(
                "Multi-level TOML editing not yet supported; use adapter-specific logic"
                    .to_string(),
            ));
        }

        // Write back with atomic write
        let mut file = AtomicWriteFile::open(config_path)?;
        file.write_all(doc.to_string().as_bytes())?;
        file.commit()?;

        Ok(())
    }

    /// Ensure integration file includes managed config via include directive.
    /// This is called by adapters to set up the include relationship.
    /// For TOML files: adds `include = "path"` line
    /// For shell scripts: adds `source /path/to/file` line
    /// For git config: adds `[include] path = ...` section
    /// Idempotent: if include already present, does nothing.
    pub fn ensure_integration_includes_managed(
        &self,
        config_path: &Path,
        _managed_path: &Path,
    ) -> Result<()> {
        if !config_path.exists() {
            // Integration file doesn't exist yet; may be created by tool on first run
            // This is not an error — tool will create it
            return Ok(());
        }

        // For now, this is a placeholder for adapters to implement per-tool logic
        // Each adapter will have its own way of ensuring the include is present
        // (some use TOML include syntax, some use environment variables, some use source commands)

        Ok(())
    }

    /// Get the base path for three-tier config
    pub fn base_path(&self) -> &Path {
        &self.base_path
    }

    fn infer_tool_name(config_path: &Path) -> String {
        let file_name = config_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("config");

        if file_name == "config" {
            config_path
                .parent()
                .and_then(|parent| parent.file_name())
                .and_then(|name| name.to_str())
                .unwrap_or("config")
                .to_string()
        } else {
            Path::new(file_name)
                .file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or(file_name)
                .trim_start_matches('.')
                .to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn test_config_manager(base_path: &Path) -> ConfigManager {
        ConfigManager {
            base_path: base_path.to_path_buf(),
            backup_root: base_path.join(".cache/slate/backups"),
        }
    }

    #[test]
    fn test_config_manager_with_env() {
        let temp = TempDir::new().unwrap();
        let env = SlateEnv::with_home(temp.path().to_path_buf());
        let cm = ConfigManager::with_env(&env).unwrap();

        // Verify base_path is set correctly
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

        // Initially no theme set
        let current = config_manager.get_current_theme().unwrap();
        assert_eq!(current, None);

        // Set theme
        config_manager
            .set_current_theme("catppuccin-mocha")
            .unwrap();

        // Verify it was set
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
        let expected = config_manager
            .managed_dir("zsh")
            .join("highlight-styles.sh")
            .to_string_lossy()
            .to_string();
        assert!(content.contains(&format!("if [ -f \"{}\" ]; then", expected)));
        assert!(content.contains(&format!("source \"{}\"", expected)));
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
        let xdg_root = base_path
            .parent()
            .unwrap()
            .to_string_lossy()
            .to_string();

        assert!(content.contains(&format!("export EZA_CONFIG_DIR=\"{}/eza\"", managed_root)));
        assert!(content.contains(&format!(
            "export LG_CONFIG_FILE=\"{managed}/lazygit/config.yml:{xdg}/lazygit/config.yml\"",
            managed = managed_root,
            xdg = xdg_root
        )));
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
        assert!(content.contains("if ! pgrep -qf \"slate-dark-mode-notify\"; then"));
        assert!(content.contains("theme --auto --quiet &!"));
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

        // Create a test config file
        let initial = r#"
palette = "old"
format = "..."
"#;
        fs::write(&config_path, initial).unwrap();

        let config_manager = test_config_manager(temp.path());

        // Edit the palette key
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

        // When file missing, should default to Solid
        let preset = config_manager.get_current_opacity_preset().unwrap();
        assert_eq!(preset, crate::opacity::OpacityPreset::Solid);
    }

    #[test]
    fn test_opacity_persistence_round_trip() {
        let temp = TempDir::new().unwrap();
        let config_manager = test_config_manager(temp.path());

        // Set opacity
        config_manager
            .set_current_opacity_preset(crate::opacity::OpacityPreset::Frosted)
            .unwrap();

        // Read it back
        let preset = config_manager.get_current_opacity_preset().unwrap();
        assert_eq!(preset, crate::opacity::OpacityPreset::Frosted);

        // Verify file content is lowercase
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

        // Initially not enabled
        let enabled = config_manager.has_fastfetch_autorun().unwrap();
        assert!(!enabled);

        // Enable it
        config_manager.enable_fastfetch_autorun().unwrap();
        let enabled = config_manager.has_fastfetch_autorun().unwrap();
        assert!(enabled);

        // Verify marker file exists
        let path = config_manager.base_path().join("autorun-fastfetch");
        assert!(path.exists());

        // Disable it
        config_manager.disable_fastfetch_autorun().unwrap();
        let enabled = config_manager.has_fastfetch_autorun().unwrap();
        assert!(!enabled);
    }

    #[test]
    fn test_fastfetch_disable_is_idempotent() {
        let temp = TempDir::new().unwrap();
        let config_manager = test_config_manager(temp.path());

        // Disable when not enabled should not error
        let result = config_manager.disable_fastfetch_autorun();
        assert!(result.is_ok());

        // Disable again should also work
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
        use tempfile::TempDir;

        let temp = TempDir::new().unwrap();
        let config_manager = test_config_manager(temp.path());

        // Create marker file
        config_manager.enable_fastfetch_autorun().unwrap();

        // Get a theme for shell integration
        let registry = crate::theme::ThemeRegistry::new().unwrap();
        let theme = registry.get("catppuccin-mocha").unwrap();

        // Write shell integration
        config_manager.write_shell_integration_file(theme).unwrap();

        // Read the generated env.zsh
        let env_zsh_path = temp.path().join("managed/shell/env.zsh");
        let content = std::fs::read_to_string(&env_zsh_path).unwrap();

        // Verify fastfetch command is present
        assert!(content.contains("if command -v fastfetch &> /dev/null; then"));
        assert!(content.contains("  fastfetch"));
        assert!(content.contains("fi"));
    }

    #[test]
    fn test_write_shell_integration_excludes_fastfetch_when_marker_absent() {
        use tempfile::TempDir;

        let temp = TempDir::new().unwrap();
        let config_manager = test_config_manager(temp.path());

        // Do NOT create marker file (fastfetch disabled by default)

        // Get a theme for shell integration
        let registry = crate::theme::ThemeRegistry::new().unwrap();
        let theme = registry.get("catppuccin-mocha").unwrap();

        // Write shell integration
        config_manager.write_shell_integration_file(theme).unwrap();

        // Read the generated env.zsh
        let env_zsh_path = temp.path().join("managed/shell/env.zsh");
        let content = std::fs::read_to_string(&env_zsh_path).unwrap();

        // Verify fastfetch conditional is NOT present
        assert!(!content.contains("if command -v fastfetch &> /dev/null; then"));
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

        assert!(content.contains("BAT_THEME=\"Tokyo Night\""));
    }

    #[test]
    fn test_refresh_shell_integration_falls_back_to_default_theme() {
        let temp = TempDir::new().unwrap();
        let config_manager = test_config_manager(temp.path());

        config_manager.set_current_theme("missing-theme").unwrap();
        config_manager.refresh_shell_integration().unwrap();

        let env_zsh_path = temp.path().join("managed/shell/env.zsh");
        let content = std::fs::read_to_string(&env_zsh_path).unwrap();

        assert!(content.contains("BAT_THEME=\"Catppuccin Mocha\""));
    }
}
