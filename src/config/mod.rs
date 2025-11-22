use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use atomic_write_file::AtomicWriteFile;
use toml_edit::DocumentMut;
use crate::error::{Result, SlateError};

/// Three-tier configuration manager.
/// Manages three tiers per /// 1. Managed tier: ~/.config/slate/managed/{tool}/ — Slate writes here (regenerates freely)
/// 2. Integration tier: ~/.config/{tool}/config — User's entry file (slate ensures it includes managed, never modifies content)
/// 3. User tier: ~/.config/slate/user/{tool}/ — User's custom overrides (slate never touches)
pub struct ConfigManager {
    base_path: PathBuf,  // ~/.config/slate
}

impl ConfigManager {
    /// Create ConfigManager with default base path.
    /// hardcode ~/.config/slate/ (macOS-only, no ProjectDirs)
    pub fn new() -> Result<Self> {
        let home = std::env::var("HOME")
            .map_err(|_| crate::error::SlateError::MissingHomeDir)?;

        let base_path = PathBuf::from(home).join(".config/slate");

        // Ensure base directory exists
        fs::create_dir_all(&base_path)?;

        Ok(Self { base_path })
    }

    /// Path to managed directory for a tool
    /// Example: ~/.config/slate/managed/ghostty
    fn managed_dir(&self, tool: &str) -> PathBuf {
        self.base_path.join("managed").join(tool)
    }

    /// Path to user override directory for a tool
    /// Example: ~/.config/slate/user/ghostty
    #[allow(dead_code)]
    fn user_dir(&self, tool: &str) -> PathBuf {
        self.base_path.join("user").join(tool)
    }

    /// Path to backup directory for a tool
    /// Example: ~/.config/slate/backups/starship
    fn backups_dir(&self, tool: &str) -> PathBuf {
        self.base_path.join("backups").join(tool)
    }

    /// Path to current theme tracking file
    /// ~/.config/slate/current — plain text file with theme ID 
    fn current_theme_path(&self) -> PathBuf {
        self.base_path.join("current")
    }

    /// Path to the user's config root.
    /// Normally this is ~/.config when base_path is ~/.config/slate.
    /// Tests may inject a temp base_path directly, in which case that directory acts as config root.
    fn config_root(&self) -> PathBuf {
        let is_standard_slate_dir = self.base_path
            .file_name()
            .and_then(|name| name.to_str())
            == Some("slate");

        if is_standard_slate_dir {
            self.base_path
                .parent()
                .unwrap_or(self.base_path.as_path())
                .to_path_buf()
        } else {
            self.base_path.clone()
        }
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
    /// Write shell integration file (env.zsh) with theme-aware content.
    /// Per , generates exports + fastfetch wrapper + zsh-highlight source.
    /// Called both during setup (to initialize) and on `slate set` (to update).
    pub fn write_shell_integration_file(&self, theme: &crate::theme::ThemeVariant) -> Result<()> {
        let mut content = String::new();
        
        // Export BAT_THEME
        content.push_str(&format!("export BAT_THEME=\"{}\"
", theme.tool_refs.bat));
        
        // Export EZA_CONFIG_DIR
        content.push_str("export EZA_CONFIG_DIR=\"$HOME/.config/slate/managed/eza\"
");
        
        // Export LG_CONFIG_FILE
        content.push_str("export LG_CONFIG_FILE=\"$HOME/.config/slate/managed/lazygit/config.yml:$HOME/.config/lazygit/config.yml\"
");
        
        // Add fastfetch wrapper function
        content.push_str("fastfetch() { command fastfetch -c ~/.config/slate/managed/fastfetch/config.jsonc \"$@\"; }
");
        
        // Add zsh-syntax-highlighting source line
        content.push_str("source ~/.config/slate/managed/zsh/highlight-styles.sh
");
        
        // Write atomically to ~/.config/slate/managed/shell/env.zsh
        self.write_managed_file("shell", "env.zsh", &content)?;
        
        Ok(())
    }


    /// Render shell integration for `slate init <shell>`.
    /// slate prints shell exports/source commands directly.
    /// Implements : environment variable exports for EnvironmentVariable strategy adapters
    /// (bat, eza, lazygit) and tool wrappers (fastfetch).
    pub fn render_shell_init(&self, shell: &str) -> Result<String> {
        use crate::theme::{ThemeRegistry, DEFAULT_THEME_ID};

        let current_theme_id = self.get_current_theme()?;
        let theme_registry = ThemeRegistry::new()?;

        let default_theme = theme_registry
            .get(DEFAULT_THEME_ID)
            .ok_or_else(|| SlateError::InvalidThemeData(format!(
                "Default theme '{}' is missing from registry",
                DEFAULT_THEME_ID
            )))?;

        let (theme, warning_comment) = match current_theme_id.as_deref() {
            Some(theme_id) => match theme_registry.get(theme_id) {
                Some(theme) => (theme, None),
                None => (
                    default_theme,
                    Some(format!(
                        "# WARNING: Current theme '{}' not found. Using default values.\n",
                        theme_id
                    )),
                ),
            },
            None => (
                default_theme,
                Some("# WARNING: No current theme set. Using default values.\n".to_string()),
            ),
        };

        let managed_root = self.base_path.join("managed");
        let config_root = self.config_root();
        let eza_config_dir = managed_root.join("eza");
        let lazygit_managed = managed_root.join("lazygit").join("config.yml");
        let lazygit_user = config_root.join("lazygit").join("config.yml");
        let fastfetch_config = managed_root.join("fastfetch").join("config.jsonc");

        let mut env_exports = String::new();
        if let Some(warning_comment) = warning_comment {
            env_exports.push_str(&warning_comment);
        }
        env_exports.push_str(&format!("export BAT_THEME=\"{}\"\n", theme.tool_refs.bat));
        env_exports.push_str(&format!(
            "export EZA_CONFIG_DIR=\"{}\"\n",
            eza_config_dir.display()
        ));
        env_exports.push_str(&format!(
            "export LG_CONFIG_FILE=\"{},{}\"\n",
            lazygit_managed.display(),
            lazygit_user.display()
        ));

        let fastfetch_wrapper = format!(
            "fastfetch() {{\n  command fastfetch -c \"{}\" \"$@\"\n}}\n",
            fastfetch_config.display()
        );

        match shell {
            "zsh" | "bash" => {
                let output = format!(
                    "# slate shell init for {}\n\
# Auto-generated by: eval \"$(slate init {})\"\n\
{}\n\
# fastfetch wrapper function\n\
{}\n",
                    shell, shell, env_exports, fastfetch_wrapper
                );
                Ok(output)
            }
            "fish" => {
                // Convert to Fish syntax
                let mut fish_init = format!(
                    "# slate shell init for fish\n\
# Auto-generated by: eval (slate init fish)\n"
                );
                
                // Convert env exports to fish syntax
                for line in env_exports.lines() {
                    if line.starts_with("export ") {
                        let rest = &line[7..]; // skip "export "
                        if let Some(eq_pos) = rest.find('=') {
                            let var_name = &rest[..eq_pos];
                            let var_value = &rest[eq_pos + 1..];
                            // Remove surrounding quotes if present
                            let var_value = var_value.trim_matches('"');
                            fish_init.push_str(&format!(
                                "set -gx {} \"{}\"\n",
                                var_name, var_value
                            ));
                        }
                    } else if !line.is_empty() {
                        // Keep comments as-is
                        fish_init.push_str(line);
                        fish_init.push('\n');
                    }
                }
                
                // Fish function wrapper
                fish_init.push_str(&format!(
                    "function fastfetch --wraps fastfetch\n  command fastfetch -c \"{}\" $argv\nend\n",
                    fastfetch_config.display()
                ));
                
                Ok(fish_init)
            }
            other => Err(SlateError::InvalidConfig(format!(
                "Unsupported shell '{}'. Use zsh, bash, or fish.",
                other
            ))),
        }
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
    pub fn edit_config_field(
        &self,
        config_path: &Path,
        keys: &[&str],
        value: &str,
    ) -> Result<()> {
        if !config_path.exists() {
            return Err(crate::error::SlateError::ConfigNotFound(
                config_path.to_string_lossy().to_string()
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
                "Multi-level TOML editing not yet supported; use adapter-specific logic".to_string()
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

impl Default for ConfigManager {
    fn default() -> Self {
        // In production, this could fail if $HOME is not set
        // Tests should handle this via environment setup
        Self::new().expect("Failed to initialize ConfigManager: $HOME not set or invalid")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_config_manager_creates_base_path() {
        // Test that ConfigManager can be created
        // Using real environment HOME for integration test
        match ConfigManager::new() {
            Ok(cm) => {
                // Verify base_path exists and is correct
                assert!(cm.base_path.ends_with(".config/slate"));
            }
            Err(_) => {
                // HOME not set in test env, that's ok
            }
        }
    }

    #[test]
    fn test_managed_file_write() {
        let temp = TempDir::new().unwrap();
        let config_manager = ConfigManager {
            base_path: temp.path().to_path_buf(),
        };

        let result = config_manager.write_managed_file(
            "ghostty",
            "colors.conf",
            "# Managed config\ncolor0 = #000000"
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
        let config_manager = ConfigManager {
            base_path: temp.path().to_path_buf(),
        };

        assert_eq!(
            config_manager.user_dir("ghostty"),
            temp.path().join("user").join("ghostty")
        );
    }

    #[test]
    fn test_current_theme_tracking() {
        let temp = TempDir::new().unwrap();
        let config_manager = ConfigManager {
            base_path: temp.path().to_path_buf(),
        };

        // Initially no theme set
        let current = config_manager.get_current_theme().unwrap();
        assert_eq!(current, None);

        // Set theme
        config_manager.set_current_theme("catppuccin-mocha").unwrap();

        // Verify it was set
        let current = config_manager.get_current_theme().unwrap();
        assert_eq!(current, Some("catppuccin-mocha".to_string()));
    }

    #[test]
    fn test_render_shell_init() {
        let temp = TempDir::new().unwrap();
        let config_manager = ConfigManager {
            base_path: temp.path().to_path_buf(),
        };

        config_manager.set_current_theme("catppuccin-mocha").unwrap();
        let shell_init = config_manager.render_shell_init("zsh").unwrap();

        // Check for header comment and shell type
        assert!(shell_init.contains("slate shell init for zsh"));
        
        // Check for environment variable exports
        assert!(shell_init.contains("export BAT_THEME=\"Catppuccin Mocha\""));
        assert!(shell_init.contains(&format!(
            "export EZA_CONFIG_DIR=\"{}\"",
            temp.path().join("managed/eza").display()
        )));
        assert!(shell_init.contains(&format!(
            "export LG_CONFIG_FILE=\"{},{}\"",
            temp.path().join("managed/lazygit/config.yml").display(),
            temp.path().join("lazygit/config.yml").display()
        )));
        
        // Check for fastfetch wrapper
        assert!(shell_init.contains("fastfetch()"));
        assert!(shell_init.contains(&temp.path().join("managed/fastfetch/config.jsonc").display().to_string()));
        
        // Verify theme is used
        assert!(shell_init.contains("Catppuccin Mocha"));
    }

    #[test]
    fn test_render_shell_init_without_current_theme_uses_default_exports() {
        let temp = TempDir::new().unwrap();
        let config_manager = ConfigManager {
            base_path: temp.path().to_path_buf(),
        };

        let shell_init = config_manager.render_shell_init("zsh").unwrap();

        assert!(shell_init.contains("# WARNING: No current theme set. Using default values."));
        assert!(shell_init.contains("export BAT_THEME=\"Catppuccin Mocha\""));
        assert!(shell_init.contains(&format!(
            "export LG_CONFIG_FILE=\"{},{}\"",
            temp.path().join("managed/lazygit/config.yml").display(),
            temp.path().join("lazygit/config.yml").display()
        )));
    }

    #[test]
    fn test_render_shell_init_with_invalid_current_theme_uses_default_exports() {
        let temp = TempDir::new().unwrap();
        let config_manager = ConfigManager {
            base_path: temp.path().to_path_buf(),
        };

        config_manager.set_current_theme("bogus-theme").unwrap();
        let shell_init = config_manager.render_shell_init("zsh").unwrap();

        assert!(shell_init.contains("# WARNING: Current theme 'bogus-theme' not found. Using default values."));
        assert!(shell_init.contains("export BAT_THEME=\"Catppuccin Mocha\""));
        assert!(shell_init.contains(&format!(
            "export EZA_CONFIG_DIR=\"{}\"",
            temp.path().join("managed/eza").display()
        )));
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

        let config_manager = ConfigManager {
            base_path: temp.path().to_path_buf(),
        };

        // Edit the palette key
        let result = config_manager.edit_config_field(
            &config_path,
            &["palette"],
            "new-palette"
        );

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

        let config_manager = ConfigManager {
            base_path: temp.path().join(".config/slate"),
        };
        fs::create_dir_all(config_manager.base_path()).unwrap();

        let backup_path = config_manager.backup_file(&config_path).unwrap();

        assert!(backup_path.starts_with(config_manager.backups_dir("starship")));
        assert!(backup_path.exists());
    }
}
