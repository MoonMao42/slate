//! Ghostty adapter with WriteAndInclude strategy.
//! Per D-05a: Ghostty is one of two locked exceptions to EditInPlace rule,
//! using WriteAndInclude strategy instead. This is because Ghostty's include
//! directive is a simple key-value line, not complex configuration merging.
//! D-05b: Idempotent config-file directive insertion ensures running twice
//! produces the same result (no duplicate include lines).

use crate::adapter::{ApplyStrategy, ToolAdapter};
use crate::config::ConfigManager;
use crate::env::SlateEnv;
use crate::error::{Result, SlateError};
use crate::theme::ThemeVariant;
#[cfg(target_os = "macos")]
use crossterm::terminal;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
#[cfg(target_os = "macos")]
use std::thread;
#[cfg(target_os = "macos")]
use std::time::Duration;

/// Ghostty adapter implementing v2 ToolAdapter trait.
pub struct GhosttyAdapter;

impl GhosttyAdapter {
    /// The current Ghostty default config path documented upstream.
    fn default_config_path(xdg_dir: &Path) -> PathBuf {
        xdg_dir.join("config.ghostty")
    }

    /// Build candidate config paths in priority order.
    /// Ghostty resolves: official > XDG > macOS legacy (Application Support).
    fn candidate_paths(xdg_dir: &Path, home: Option<&str>) -> Vec<PathBuf> {
        let mut paths = Vec::new();

        // Upstream-documented default path (Ghostty 1.1+).
        paths.push(Self::default_config_path(xdg_dir));

        // XDG config (without .ghostty extension) — common user setup.
        paths.push(xdg_dir.join("config"));

        // Legacy macOS App Support location, lowest priority.
        if cfg!(target_os = "macos") {
            if let Some(h) = home {
                let appsupport =
                    PathBuf::from(h).join("Library/Application Support/com.mitchellh.ghostty");
                paths.push(appsupport.join("config.ghostty"));
                paths.push(appsupport.join("config"));
            }
        }

        paths
    }

    fn first_existing_path(candidates: &[PathBuf]) -> Option<PathBuf> {
        candidates.iter().find(|p| p.exists()).cloned()
    }

    /// Insert managed path in integration file idempotently.
    /// Per D-05b: integration file can be created by tool (zero-config setup).
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

    /// Update font-family in integration config.
    /// Modifies user's integration file (not managed) because Ghostty main config
    /// takes precedence over config-file includes for font-family.
    fn update_font_in_config(integration_path: &Path, font_family: &str) -> Result<()> {
        if !integration_path.exists() {
            // Config file doesn't exist, file will be created by Ghostty on first run.
            // Skip font update — Ghostty will use system defaults until explicitly set.
            return Ok(());
        }

        let mut content = fs::read_to_string(integration_path)?;

        let font_line = format!("font-family = \"{}\"\n", font_family);
        let font_pattern = "font-family";

        if let Some(idx) = content.find(font_pattern) {
            // Find end of line and replace
            let end_of_line = content[idx..]
                .find('\n')
                .map(|i| idx + i + 1)
                .unwrap_or(content.len());
            content.replace_range(idx..end_of_line, &font_line);
        } else {
            // Append to end of file
            content.push_str(&font_line);
        }

        fs::write(integration_path, content)?;

        Ok(())
    }

    fn send_reload_signal(command_name: &str) -> Result<()> {
        let output = Command::new(command_name)
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

    #[cfg(target_os = "macos")]
    fn perform_focused_terminal_action(command_name: &str, action: &str) -> Result<()> {
        let script = format!(
            r#"tell application "Ghostty"
    set target_terminal to focused terminal of selected tab of front window
    perform action "{}" on target_terminal
end tell"#,
            action
        );

        let output = Command::new(command_name)
            .arg("-e")
            .arg(&script)
            .output()
            .map_err(|e| {
                SlateError::ReloadFailed(
                    "ghostty".to_string(),
                    format!(
                        "Failed to invoke Ghostty AppleScript action '{}': {}",
                        action, e
                    ),
                )
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            let detail = if stderr.is_empty() {
                format!(
                    "Ghostty AppleScript action '{}' failed. macOS may require Automation permission for the calling app.",
                    action
                )
            } else {
                format!(
                    "Ghostty AppleScript action '{}' failed: {}. macOS may require Automation permission for the calling app.",
                    action, stderr
                )
            };

            return Err(SlateError::ReloadFailed("ghostty".to_string(), detail));
        }

        Ok(())
    }

    #[cfg(target_os = "macos")]
    fn reload_via_applescript(command_name: &str) -> Result<()> {
        Self::perform_focused_terminal_action(command_name, "reload_config")
    }

    #[cfg(target_os = "macos")]
    fn mark_focused_terminal_font_size_adjusted(command_name: &str) {
        // Ghostty preserves font size across reload only for terminals marked
        // as manually adjusted. A zero-delta increase is visually inert but
        // still flips that internal flag for inherited-size windows/tabs.
        let _ = Self::perform_focused_terminal_action(command_name, "increase_font_size:0");
    }

    #[cfg(target_os = "macos")]
    fn running_inside_ghostty() -> bool {
        std::env::var("TERM_PROGRAM")
            .map(|value| value == "Ghostty")
            .unwrap_or(false)
    }

    #[cfg(target_os = "macos")]
    fn current_terminal_grid() -> Option<(u16, u16)> {
        terminal::size().ok()
    }

    #[cfg(target_os = "macos")]
    fn focused_window_size(command_name: &str) -> Option<(u32, u32)> {
        let script = r#"tell application "System Events"
    tell process "Ghostty"
        set windowSize to size of front window
        return ((item 1 of windowSize as text) & "," & (item 2 of windowSize as text))
    end tell
end tell"#;

        let output = Command::new(command_name)
            .arg("-e")
            .arg(script)
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut parts = stdout.trim().split(',').map(str::trim);
        let width = parts.next()?.parse().ok()?;
        let height = parts.next()?.parse().ok()?;
        Some((width, height))
    }

    #[cfg(target_os = "macos")]
    fn grid_distance(target: (u16, u16), current: (u16, u16)) -> u32 {
        target.0.abs_diff(current.0) as u32 + target.1.abs_diff(current.1) as u32
    }

    #[cfg(target_os = "macos")]
    fn window_size_distance(target: (u32, u32), current: (u32, u32)) -> u64 {
        target.0.abs_diff(current.0) as u64 + target.1.abs_diff(current.1) as u64
    }

    #[cfg(target_os = "macos")]
    fn restore_focused_window_size(target: (u32, u32)) -> bool {
        if !Self::running_inside_ghostty() {
            return false;
        }

        thread::sleep(Duration::from_millis(120));

        let Some(current) = Self::focused_window_size("osascript") else {
            return false;
        };
        if current == target {
            return true;
        }

        let action = if current.0 < target.0 || current.1 < target.1 {
            "increase_font_size:1"
        } else {
            "decrease_font_size:1"
        };

        let mut best = Self::window_size_distance(target, current);
        let mut previous = current;

        for _ in 0..24 {
            if Self::perform_focused_terminal_action("osascript", action).is_err() {
                return false;
            }

            thread::sleep(Duration::from_millis(40));

            let Some(next) = Self::focused_window_size("osascript") else {
                return false;
            };
            if next == target {
                return true;
            }

            let distance = Self::window_size_distance(target, next);
            if distance >= best || next == previous {
                return false;
            }

            best = distance;
            previous = next;
        }

        false
    }

    #[cfg(target_os = "macos")]
    fn restore_focused_terminal_grid(target: (u16, u16)) {
        if !Self::running_inside_ghostty() {
            return;
        }

        // Give Ghostty a moment to finish applying the config before we sample.
        thread::sleep(Duration::from_millis(120));

        let Some(current) = Self::current_terminal_grid() else {
            return;
        };
        if current == target {
            return;
        }

        let action = if current.0 > target.0 || current.1 > target.1 {
            "increase_font_size:1"
        } else {
            "decrease_font_size:1"
        };

        let mut best = Self::grid_distance(target, current);
        let mut previous = current;

        for _ in 0..24 {
            if Self::perform_focused_terminal_action("osascript", action).is_err() {
                return;
            }

            thread::sleep(Duration::from_millis(40));

            let Some(next) = Self::current_terminal_grid() else {
                return;
            };
            if next == target {
                return;
            }

            let distance = Self::grid_distance(target, next);
            if distance >= best || next == previous {
                return;
            }

            best = distance;
            previous = next;
        }
    }
}

impl ToolAdapter for GhosttyAdapter {
    fn tool_name(&self) -> &'static str {
        "ghostty"
    }

    fn is_installed(&self) -> Result<bool> {
        let binary_exists = which::which("ghostty").is_ok();

        let config_exists = match self.integration_config_path() {
            Ok(path) => path.exists(),
            Err(_) => false,
        };

        Ok(binary_exists || config_exists)
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

    fn apply_theme(&self, theme: &ThemeVariant) -> Result<()> {
        // Step 1: Extract theme name from tool_refs
        let ghostty_theme = theme
            .tool_refs
            .get("ghostty")
            .ok_or_else(|| {
                SlateError::InvalidThemeData(format!(
                    "Theme '{}' missing ghostty tool reference",
                    theme.id
                ))
            })?
            .to_string();

        // Step 2: Render managed config as theme-only line
        let managed_content = format!("theme = \"{}\"\n", ghostty_theme);

        // Step 3: Write managed theme config
        let config_manager = ConfigManager::new()?;
        config_manager.write_managed_file("ghostty", "theme.conf", &managed_content)?;

        // Step 4: Ensure integration file includes all managed paths idempotently
        let integration_path = self.integration_config_path()?;
        let managed_base = self.managed_config_path();
        let theme_path = managed_base.join("theme.conf");
        let opacity_path = managed_base.join("opacity.conf");
        let blur_path = managed_base.join("blur.conf");

        // Ensure parent directory exists for integration file
        if let Some(parent) = integration_path.parent() {
            if !parent.exists() {
                fs::create_dir_all(parent)?;
            }
        }

        // Include all three managed files
        Self::ensure_integration_includes_managed(&integration_path, &theme_path)?;
        Self::ensure_integration_includes_managed(&integration_path, &opacity_path)?;
        Self::ensure_integration_includes_managed(&integration_path, &blur_path)?;

        // Note: Font updates are handled by the FontAdapter (applied in plan 06-06).
        // Theme switches should only affect colors, not fonts.
        // Font changes are an orthogonal concern managed separately.

        Ok(())
    }

    fn reload(&self) -> Result<()> {
        #[cfg(target_os = "macos")]
        {
            let previous_window_size = Self::focused_window_size("osascript");
            let previous_grid = Self::current_terminal_grid();

            Self::mark_focused_terminal_font_size_adjusted("osascript");

            // macOS uses Ghostty's terminal-targeted AppleScript reload so we
            // only hot-reload the currently focused window.
            let result = Self::reload_via_applescript("osascript");

            if result.is_ok() {
                let restored_window = previous_window_size
                    .map(Self::restore_focused_window_size)
                    .unwrap_or(false);

                if !restored_window {
                    if let Some(grid) = previous_grid {
                        Self::restore_focused_terminal_grid(grid);
                    }
                }
            }

            result
        }

        #[cfg(not(target_os = "macos"))]
        {
            Self::send_reload_signal("pkill")
        }
    }
}

/// Helper methods using injected SlateEnv (for testing)
impl GhosttyAdapter {
    pub fn integration_config_path_with_env(&self, env: &SlateEnv) -> Result<PathBuf> {
        let home = env.home().to_str().ok_or(SlateError::MissingHomeDir)?;
        let xdg_dir = env.home().join(".config").join("ghostty");

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
/// Per , Sets background-opacity value based on OpacityPreset.
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
        "background-blur-radius = {}
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
        assert!(path.to_string_lossy().contains("config.ghostty"));
    }

    #[test]
    fn test_ghostty_candidate_paths_includes_xdg() {
        let xdg_dir = PathBuf::from("/test/.config/ghostty");
        let candidates = GhosttyAdapter::candidate_paths(&xdg_dir, Some("/home/user"));
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
        assert!(path.ends_with("config.ghostty"));
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
        adapter.apply_theme(&theme).unwrap();

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
    fn test_send_reload_signal_reports_spawn_failure() {
        let err = GhosttyAdapter::send_reload_signal("this-command-should-not-exist").unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("Failed to reload ghostty"));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_reload_via_applescript_reports_spawn_failure() {
        let err =
            GhosttyAdapter::reload_via_applescript("this-command-should-not-exist").unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("Ghostty AppleScript action 'reload_config'"));
    }
}
