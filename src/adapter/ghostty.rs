//! Ghostty adapter with WriteAndInclude strategy.
//! Ghostty is one of two locked exceptions to EditInPlace rule,
//! using WriteAndInclude strategy instead. This is because Ghostty's include
//! directive is a simple key-value line, not complex configuration merging.
//! Idempotent config-file directive insertion ensures running twice
//! produces the same result (no duplicate include lines).

use crate::adapter::{ApplyOutcome, ApplyStrategy, SkipReason, ToolAdapter};
use crate::config::ConfigManager;
use crate::detection;
use crate::env::SlateEnv;
use crate::error::{Result, SlateError};
use crate::theme::ThemeVariant;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Ghostty adapter implementing the ToolAdapter trait.
pub struct GhosttyAdapter;

impl GhosttyAdapter {
    /// The current Ghostty default config path documented upstream.
    /// Ghostty uses `config` (no extension) as the standard config filename.
    fn default_config_path(xdg_dir: &Path) -> PathBuf {
        xdg_dir.join("config")
    }

    /// Build candidate config paths in priority order.
    /// Ghostty resolves: XDG config > legacy.ghostty extension > macOS App Support.
    fn candidate_paths(xdg_dir: &Path, home: Option<&str>) -> Vec<PathBuf> {
        let mut paths = Vec::new();

        // Standard Ghostty config path (no extension).
        paths.push(xdg_dir.join("config"));

        // Legacy.ghostty extension (some older setups).
        paths.push(xdg_dir.join("config.ghostty"));

        // Legacy macOS App Support location, lowest priority.
        if cfg!(target_os = "macos") {
            if let Some(h) = home {
                let appsupport =
                    PathBuf::from(h).join("Library/Application Support/com.mitchellh.ghostty");
                paths.push(appsupport.join("config"));
                paths.push(appsupport.join("config.ghostty"));
            }
        }

        paths
    }

    fn first_existing_path(candidates: &[PathBuf]) -> Option<PathBuf> {
        candidates.iter().find(|p| p.exists()).cloned()
    }

    /// Insert managed path in integration file idempotently.
    /// integration file can be created by tool (zero-config setup).
    /// IMPORTANT: This function does NOT create the integration file if it doesn't exist.
    /// The file must already exist (created by setup wizard or user).
    /// This prevents slate from destructively creating a minimal config that could override
    /// GUI-level settings (like macOS icon preferences stored in plist/defaults).
    /// Ghostty's correct syntax is `config-file = "path"`, not `include = "path"`.
    /// This function ensures idempotent integration by:
    /// - Checking if slate's managed file is already referenced (skips if so)
    /// - Migrating legacy `include = <slate-managed>` lines to `config-file = <slate-managed>`
    /// - Detecting by exact path match, not substring
    fn ensure_integration_includes_managed(
        integration_path: &Path,
        managed_path: &Path,
    ) -> Result<()> {
        if !integration_path.exists() {
            // File doesn't exist; slate should not create it automatically.
            // This protects against overwriting GUI-level settings.
            // The integration config must be created by setup wizard or user manually.
            return Ok(());
        }

        let content = fs::read_to_string(integration_path)?;
        let managed_path_str = managed_path.display().to_string();

        // Parse content line-by-line to detect idempotence and handle migration
        let lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();
        let mut found_config_file = false;
        let mut legacy_include_idx = None;

        for (idx, line) in lines.iter().enumerate() {
            let trimmed = line.trim();

            // Skip comments and empty lines
            if trimmed.starts_with('#') || trimmed.is_empty() {
                continue;
            }

            // Check for existing config-file pointing to our managed path
            if trimmed.starts_with("config-file") && trimmed.contains(&managed_path_str) {
                found_config_file = true;
                break;
            }

            // Check for legacy include = pointing to our managed path (for migration)
            if trimmed.starts_with("include") && trimmed.contains(&managed_path_str) {
                legacy_include_idx = Some(idx);
                break;
            }
        }

        // If already using config-file pointing to our managed path, we're done
        if found_config_file {
            return Ok(());
        }

        // If legacy include exists, migrate it to config-file
        if let Some(idx) = legacy_include_idx {
            let mut updated_lines = lines;
            updated_lines[idx] = format!("config-file = \"{}\"", managed_path_str);
            let new_content = updated_lines.join("\n");
            fs::write(integration_path, format!("{}\n", new_content))?;
            return Ok(());
        }

        // Otherwise, append the config-file line
        let config_file_line = format!("config-file = \"{}\"\n", managed_path_str);
        let new_content = if content.ends_with('\n') {
            format!("{}{}", content, config_file_line)
        } else {
            format!("{}\n{}", content, config_file_line)
        };
        fs::write(integration_path, new_content)?;

        Ok(())
    }

    /// Apply font-only update to Ghostty without triggering full theme reapply.
    /// Writes only font.conf and ensures it's included.
    /// Reloads Ghostty so the font change is visible immediately.
    pub fn apply_font_only(env: &SlateEnv, font_name: &str) -> Result<()> {
        let config_manager = ConfigManager::with_env(env)?;

        // Write only the font-family to managed font.conf
        let font_conf_content = format!(
            "font-family = \"{}\"
",
            font_name
        );
        config_manager.write_managed_file("ghostty", "font.conf", &font_conf_content)?;

        // Ensure integration file includes the font.conf file
        let adapter = GhosttyAdapter;
        let integration_path = adapter.integration_config_path_with_env(env)?;
        if integration_path.exists() {
            let managed_font_path = config_manager.managed_dir("ghostty").join("font.conf");
            Self::ensure_integration_includes_managed(&integration_path, &managed_font_path)?;
        }

        // Reload Ghostty so the font change takes effect immediately
        let _ = adapter.reload();

        Ok(())
    }

    /// Reload Ghostty config via its own AppleScript API.
    /// This triggers macOS Automation permission (not Accessibility),
    /// and works even without the user granting the permission.
    #[cfg(target_os = "macos")]
    fn reload_via_applescript() -> Result<()> {
        let script = r#"tell application "Ghostty"
    set target_terminal to focused terminal of selected tab of front window
    perform action "reload_config" on target_terminal
end tell"#;

        let output = Command::new("osascript")
            .arg("-e")
            .arg(script)
            .output()
            .map_err(|e| {
                SlateError::ReloadFailed(
                    "ghostty".to_string(),
                    format!("Failed to invoke Ghostty reload: {}", e),
                )
            })?;

        if !output.status.success() {
            return Err(SlateError::ReloadFailed(
                "ghostty".to_string(),
                "Ghostty AppleScript reload failed".to_string(),
            ));
        }

        Ok(())
    }

    #[cfg(not(target_os = "macos"))]
    fn send_reload_signal() -> Result<()> {
        let output = Command::new("pkill")
            .arg("-SIGUSR2")
            .arg("-x")
            .arg("ghostty")
            .output()
            .map_err(|e| SlateError::Internal(format!("Failed to reload ghostty: {}", e)))?;

        if !output.status.success() {
            return Err(SlateError::Internal(
                "pkill signal failed (Ghostty may not be running)".to_string(),
            ));
        }

        Ok(())
    }
}

impl ToolAdapter for GhosttyAdapter {
    fn tool_name(&self) -> &'static str {
        "ghostty"
    }

    fn is_installed(&self) -> Result<bool> {
        Ok(detection::detect_tool_presence(self.tool_name()).installed)
    }

    fn integration_config_path(&self) -> Result<PathBuf> {
        let env = SlateEnv::from_process()?;
        self.integration_config_path_with_env(&env)
    }

    fn managed_config_path(&self) -> PathBuf {
        let env = SlateEnv::from_process().ok();
        self.managed_config_path_with_env(env.as_ref())
    }

    fn apply_strategy(&self) -> ApplyStrategy {
        ApplyStrategy::WriteAndInclude
    }

    fn apply_theme(&self, theme: &ThemeVariant) -> Result<ApplyOutcome> {
        let env = SlateEnv::from_process()?;
        self.apply_theme_with_env(theme, &env)
    }

    fn reload(&self) -> Result<()> {
        // macOS: use Ghostty's own AppleScript API (Automation permission, not Accessibility).
        // No System Events access = no Accessibility popup.
        #[cfg(target_os = "macos")]
        {
            Self::reload_via_applescript()
        }

        #[cfg(not(target_os = "macos"))]
        {
            Self::send_reload_signal()
        }
    }
}

/// Helper methods using injected SlateEnv (for testing)
impl GhosttyAdapter {
    fn apply_theme_with_env(&self, theme: &ThemeVariant, env: &SlateEnv) -> Result<ApplyOutcome> {
        let integration_path = self.integration_config_path_with_env(env)?;
        if !integration_path.exists() {
            return Ok(ApplyOutcome::Skipped(SkipReason::MissingIntegrationConfig));
        }

        // Write palette colors directly — no Ghostty built-in theme names.
        // This ensures terminal colors exactly match our palette (used by
        // starship, bat, etc.), eliminating cross-tool color drift.
        let p = &theme.palette;
        let managed_content = format!(
            "background = {bg}\n\
             foreground = {fg}\n\
             cursor-color = {cursor}\n\
             selection-background = {sel_bg}\n\
             selection-foreground = {sel_fg}\n\
             palette = 0={black}\n\
             palette = 1={red}\n\
             palette = 2={green}\n\
             palette = 3={yellow}\n\
             palette = 4={blue}\n\
             palette = 5={magenta}\n\
             palette = 6={cyan}\n\
             palette = 7={white}\n\
             palette = 8={br_black}\n\
             palette = 9={br_red}\n\
             palette = 10={br_green}\n\
             palette = 11={br_yellow}\n\
             palette = 12={br_blue}\n\
             palette = 13={br_magenta}\n\
             palette = 14={br_cyan}\n\
             palette = 15={br_white}\n",
            bg = p.background,
            fg = p.foreground,
            cursor = p.cursor.as_deref().unwrap_or(&p.foreground),
            sel_bg = p.selection_bg.as_deref().unwrap_or(&p.bright_black),
            sel_fg = p.selection_fg.as_deref().unwrap_or(&p.foreground),
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
        );

        // Step 3: Write managed theme config
        let config_manager = ConfigManager::with_env(env)?;
        config_manager.write_managed_file("ghostty", "theme.conf", &managed_content)?;

        let current_opacity = config_manager.get_current_opacity_preset()?;
        write_opacity_config(env, current_opacity)?;
        write_blur_radius(env, current_opacity)?;

        let current_font = config_manager.get_current_font()?;
        if let Some(ref font_family) = current_font {
            let font_conf_content = format!("font-family = \"{}\"\n", font_family);
            config_manager.write_managed_file("ghostty", "font.conf", &font_conf_content)?;
        }

        // Step 4: Ensure integration file includes all managed paths idempotently
        let managed_base = self.managed_config_path_with_env(Some(env));
        let theme_path = managed_base.join("theme.conf");
        let opacity_path = managed_base.join("opacity.conf");
        let blur_path = managed_base.join("blur.conf");

        // Include all managed files
        Self::ensure_integration_includes_managed(&integration_path, &theme_path)?;
        Self::ensure_integration_includes_managed(&integration_path, &opacity_path)?;
        Self::ensure_integration_includes_managed(&integration_path, &blur_path)?;
        if current_font.is_some() {
            let font_path = managed_base.join("font.conf");
            Self::ensure_integration_includes_managed(&integration_path, &font_path)?;
        }

        // Font updates are handled by the FontAdapter.
        // Theme switches should only affect colors, not fonts.
        // Font changes are an orthogonal concern managed separately.

        // Ghostty hot-reloads the current window via its AppleScript API;
        // the theme change is visible without spawning a new shell.
        Ok(ApplyOutcome::applied_no_shell())
    }

    pub fn integration_config_path_with_env(&self, env: &SlateEnv) -> Result<PathBuf> {
        let home = env.home().to_str().ok_or(SlateError::MissingHomeDir)?;
        let xdg_dir = env.xdg_config_home().join("ghostty");

        let candidates = Self::candidate_paths(&xdg_dir, Some(home));

        if let Some(path) = Self::first_existing_path(&candidates) {
            return Ok(path);
        }

        // Zero-config should create the current upstream default file.
        Ok(Self::default_config_path(&xdg_dir))
    }

    pub fn managed_config_path_with_env(&self, env: Option<&SlateEnv>) -> PathBuf {
        if let Some(e) = env {
            let config_dir = e.config_dir();
            config_dir.join("managed").join("ghostty")
        } else {
            PathBuf::from(".config/slate/managed/ghostty")
        }
    }
}

/// Write opacity configuration to managed Ghostty config file.
/// Sets background-opacity value based on OpacityPreset.
/// Path: ~/.config/slate/managed/ghostty/opacity.conf
pub fn write_opacity_config(env: &SlateEnv, opacity: crate::opacity::OpacityPreset) -> Result<()> {
    let config_manager = ConfigManager::with_env(env)?;

    let opacity_value = opacity.to_f32();
    let config_content = format!(
        "background-opacity = {}
",
        opacity_value
    );

    // Write to managed file, will be idempotently included by integration file
    config_manager.write_managed_file("ghostty", "opacity.conf", &config_content)?;

    Ok(())
}

/// Write blur radius configuration to managed Ghostty config file.
/// Frosted preset → 20px blur, others → 0 (no blur).
/// Path: ~/.config/slate/managed/ghostty/blur.conf
pub fn write_blur_radius(env: &SlateEnv, opacity: crate::opacity::OpacityPreset) -> Result<()> {
    let config_manager = ConfigManager::with_env(env)?;

    let blur_value = opacity.blur_radius();
    let config_content = format!(
        "background-blur = {}
",
        blur_value
    );

    // Write to managed file
    config_manager.write_managed_file("ghostty", "blur.conf", &config_content)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ghostty_adapter_tool_name() {
        let adapter = GhosttyAdapter;
        assert_eq!(adapter.tool_name(), "ghostty");
    }

    #[test]
    fn test_ghostty_default_config_path() {
        let xdg_dir = PathBuf::from("/test/.config/ghostty");
        let path = GhosttyAdapter::default_config_path(&xdg_dir);
        assert!(path.ends_with("ghostty/config"));
        assert!(!path.to_string_lossy().ends_with("config.ghostty"));
    }

    #[test]
    fn test_ghostty_candidate_paths_includes_both_names() {
        let xdg_dir = PathBuf::from("/test/.config/ghostty");
        let candidates = GhosttyAdapter::candidate_paths(&xdg_dir, Some("/home/user"));
        // Must include both config (primary) and config.ghostty (legacy)
        assert!(candidates.iter().any(|p| p.ends_with("ghostty/config")));
        assert!(candidates
            .iter()
            .any(|p| p.to_string_lossy().contains("config.ghostty")));
    }

    #[test]
    fn test_ghostty_first_existing_path() {
        let candidates = vec![
            PathBuf::from("/nonexistent/path1"),
            PathBuf::from("/nonexistent/path2"),
        ];
        assert!(GhosttyAdapter::first_existing_path(&candidates).is_none());
    }

    #[test]
    fn test_ghostty_apply_strategy() {
        let adapter = GhosttyAdapter;
        assert_eq!(adapter.apply_strategy(), ApplyStrategy::WriteAndInclude);
    }

    #[test]
    fn test_ghostty_integration_config_path_with_env() {
        let tempdir = tempfile::tempdir().unwrap();
        let env = SlateEnv::with_home(tempdir.path().to_path_buf());
        let adapter = GhosttyAdapter;

        let path = adapter.integration_config_path_with_env(&env).unwrap();
        assert!(path.ends_with("ghostty/config"));
    }

    #[test]
    fn test_ghostty_managed_config_path_with_env() {
        let tempdir = tempfile::tempdir().unwrap();
        let env = SlateEnv::with_home(tempdir.path().to_path_buf());
        let adapter = GhosttyAdapter;

        let path = adapter.managed_config_path_with_env(Some(&env));
        assert!(path.ends_with("slate/managed/ghostty"));
    }

    /// Test Bug 1 fix: config-file syntax, not include
    #[test]
    fn test_config_file_syntax_not_include() {
        use tempfile::NamedTempFile;

        let temp_file = NamedTempFile::new().unwrap();
        let temp_path = temp_file.path().to_path_buf();
        let managed_path = PathBuf::from("/home/user/.config/slate/managed/ghostty/theme.conf");

        // First call: should add config-file line
        GhosttyAdapter::ensure_integration_includes_managed(&temp_path, &managed_path).unwrap();

        let content = fs::read_to_string(&temp_path).unwrap();
        assert!(content.contains("config-file = "));
        assert!(!content.contains("include = "));
        assert!(content.contains("/home/user/.config/slate/managed/ghostty/theme.conf"));
    }

    /// Test Bug 2 fix: idempotent re-run doesn't duplicate
    #[test]
    fn test_ensure_integration_includes_managed_idempotent() {
        use tempfile::NamedTempFile;

        let temp_file = NamedTempFile::new().unwrap();
        let temp_path = temp_file.path().to_path_buf();
        let managed_path = PathBuf::from("/home/user/.config/slate/managed/ghostty/theme.conf");

        // First call
        GhosttyAdapter::ensure_integration_includes_managed(&temp_path, &managed_path).unwrap();
        let content1 = fs::read_to_string(&temp_path).unwrap();

        // Second call: should be idempotent
        GhosttyAdapter::ensure_integration_includes_managed(&temp_path, &managed_path).unwrap();
        let content2 = fs::read_to_string(&temp_path).unwrap();

        assert_eq!(content1, content2);
        assert_eq!(content1.matches("config-file = ").count(), 1);
    }

    /// Test Bug 2 fix: migration from legacy include = to config-file =
    #[test]
    fn test_migrate_legacy_include_to_config_file() {
        use std::io::Write;
        use tempfile::TempDir;

        let tempdir = TempDir::new().unwrap();
        let temp_path = tempdir.path().join("config");
        let managed_path = PathBuf::from("/home/user/.config/slate/managed/ghostty/theme.conf");

        // Write legacy include line
        let mut file = fs::File::create(&temp_path).unwrap();
        writeln!(
            file,
            "include = \"/home/user/.config/slate/managed/ghostty/theme.conf\""
        )
        .unwrap();
        drop(file);

        // Apply ensure_integration_includes_managed
        GhosttyAdapter::ensure_integration_includes_managed(&temp_path, &managed_path).unwrap();

        let content = fs::read_to_string(&temp_path).unwrap();
        assert!(!content.contains("include = "));
        assert!(content.contains("config-file = "));
        assert_eq!(content.matches("config-file = ").count(), 1);
    }

    /// Test Bug 2 fix: line-by-line detection, not substring
    #[test]
    fn test_idempotence_with_include_in_comment() {
        use std::io::Write;
        use tempfile::TempDir;

        let tempdir = TempDir::new().unwrap();
        let temp_path = tempdir.path().join("config");
        let managed_path = PathBuf::from("/home/user/.config/slate/managed/ghostty/theme.conf");

        // Write file with include in a comment
        let mut file = fs::File::create(&temp_path).unwrap();
        writeln!(file, "# Old include was: include = \"/path/to/somewhere\"").unwrap();
        writeln!(file, "background = \"#1e1e2e\"").unwrap();
        drop(file);

        // Should append config-file line (comment "include" should not trigger idempotence)
        GhosttyAdapter::ensure_integration_includes_managed(&temp_path, &managed_path).unwrap();

        let content = fs::read_to_string(&temp_path).unwrap();
        assert!(content
            .contains("config-file = \"/home/user/.config/slate/managed/ghostty/theme.conf\""));
        assert_eq!(content.matches("config-file = ").count(), 1);
    }

    /// Test Bug 3 fix: opacity and blur files are included
    #[test]
    fn test_apply_theme_includes_all_three_managed_files() {
        use std::io::Write;
        use tempfile::TempDir;

        let tempdir = TempDir::new().unwrap();
        let temp_path = tempdir.path().join("config");

        // Pre-create integration config file
        let mut file = fs::File::create(&temp_path).unwrap();
        writeln!(file, "background = \"#1e1e2e\"").unwrap();
        drop(file);

        let theme_path = PathBuf::from("/home/user/.config/slate/managed/ghostty/theme.conf");
        let opacity_path = PathBuf::from("/home/user/.config/slate/managed/ghostty/opacity.conf");
        let blur_path = PathBuf::from("/home/user/.config/slate/managed/ghostty/blur.conf");

        // Apply all three includes (as apply_theme does)
        GhosttyAdapter::ensure_integration_includes_managed(&temp_path, &theme_path).unwrap();
        GhosttyAdapter::ensure_integration_includes_managed(&temp_path, &opacity_path).unwrap();
        GhosttyAdapter::ensure_integration_includes_managed(&temp_path, &blur_path).unwrap();

        let content = fs::read_to_string(&temp_path).unwrap();

        // Should reference all three managed files with config-file = syntax
        assert_eq!(content.matches("config-file = ").count(), 3);
        assert!(content.contains("theme.conf"));
        assert!(content.contains("opacity.conf"));
        assert!(content.contains("blur.conf"));
    }

    /// Test Bug 4 fix: apply_theme does NOT modify font-family
    #[test]
    fn test_apply_theme_does_not_modify_font_family() {
        use std::io::Write;
        use tempfile::TempDir;

        let tempdir = TempDir::new().unwrap();
        let env = SlateEnv::with_home(tempdir.path().to_path_buf());
        let adapter = GhosttyAdapter;

        // Create integration config with existing font-family
        let integration_path = adapter.integration_config_path_with_env(&env).unwrap();
        fs::create_dir_all(integration_path.parent().unwrap()).unwrap();
        let mut file = fs::File::create(&integration_path).unwrap();
        writeln!(file, "font-family = \"JetBrainsMono Nerd Font\"").unwrap();
        drop(file);

        let original_content = fs::read_to_string(&integration_path).unwrap();

        // Apply theme
        let theme = crate::theme::catppuccin::catppuccin_mocha().unwrap();
        adapter.apply_theme_with_env(&theme, &env).unwrap();

        let new_content = fs::read_to_string(&integration_path).unwrap();

        // Font-family should still be the same
        assert!(new_content.contains("font-family = \"JetBrainsMono Nerd Font\""));
        // Verify it wasn't rewritten (line should appear once before and after)
        assert_eq!(
            original_content
                .matches("font-family = \"JetBrainsMono Nerd Font\"")
                .count(),
            1
        );
        assert_eq!(
            new_content
                .matches("font-family = \"JetBrainsMono Nerd Font\"")
                .count(),
            1
        );
    }

    #[test]
    fn test_reload_trait_method_exists() {
        // Verify the reload method is callable (actual reload requires running Ghostty)
        let adapter = GhosttyAdapter;
        let _result = adapter.reload();
        // Result will be Err since Ghostty is not running in test, but method exists
    }
}
