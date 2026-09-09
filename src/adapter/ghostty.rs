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
use crate::theme::{ThemeAppearance, ThemeVariant};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

mod paths;
pub(crate) mod references;

#[cfg(test)]
#[path = "ghostty/reference_tests.rs"]
mod reference_tests;

/// Ghostty adapter implementing the ToolAdapter trait.
pub struct GhosttyAdapter;

const RELOAD_LIMITS: crate::platform::process_output::Limits =
    crate::platform::process_output::Limits {
        timeout: std::time::Duration::from_secs(3),
        max_output: 16 * 1024,
    };

fn run_reload(
    command: &mut Command,
    limits: crate::platform::process_output::Limits,
) -> Result<()> {
    use crate::platform::process_output::{capture, Completion};
    let output = capture(command, limits).map_err(|error| {
        SlateError::ReloadFailed(
            "ghostty".into(),
            format!("Could not start or observe Ghostty reload: {error}"),
        )
    })?;
    let reason = match output.completion {
        Completion::Exited(status) if status.success() => return Ok(()),
        Completion::Exited(status) => format!("command exited with {status}"),
        Completion::TimedOut => "command exceeded its deadline".into(),
        Completion::OutputLimit => "command exceeded its output limit".into(),
    };
    Err(SlateError::ReloadFailed("ghostty".into(), format!(
        "Ghostty reload was not confirmed: {reason}. It may already have applied; check the window and reload its config manually if needed. Native output omitted."
    )))
}

impl GhosttyAdapter {
    fn render_config_file_line(managed_path: &Path, with_newline: bool) -> Vec<u8> {
        let mut line = format!("config-file = \"{}\"", managed_path.display()).into_bytes();
        if with_newline {
            line.push(b'\n');
        }
        line
    }

    /// Slate's zero-config write target, including on macOS. This is not
    /// Ghostty's own template-creation policy (which prefers App Support there).
    fn default_config_path(xdg_dir: &Path) -> PathBuf {
        xdg_dir.join("config.ghostty")
    }

    fn last_existing_path(candidates: &[PathBuf]) -> Option<PathBuf> {
        candidates.iter().rev().find(|p| p.exists()).cloned()
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

        let content = fs::read(integration_path)?;
        let updated = Self::font_include_content(&content, managed_path);
        if updated != content {
            crate::config::preview_write::write_legacy(integration_path, &updated)?;
        }
        Ok(())
    }

    /// Pure byte transform shared by single-adapter and prepared font changes.
    pub(crate) fn font_include_content(content: &[u8], managed_path: &Path) -> Vec<u8> {
        use references::Directive;
        let managed = managed_path.to_string_lossy();

        // Inspect without allocating one buffer per line on the no-op path.
        let mut found_config_file = false;
        let mut legacy_include_idx = None;

        for (idx, (_, directive)) in references::literal_lines(content).enumerate() {
            match directive {
                Directive::Reset => {
                    found_config_file = false;
                    legacy_include_idx = None;
                }
                Directive::Path { value, legacy, .. } if value == managed => {
                    if legacy {
                        legacy_include_idx = Some(idx);
                    } else {
                        found_config_file = true;
                    }
                }
                _ => {}
            }
        }

        // If already using config-file pointing to our managed path, we're done
        if found_config_file {
            return content.to_vec();
        }

        // If legacy include exists, migrate it to config-file
        if let Some(idx) = legacy_include_idx {
            let mut updated_lines: Vec<Vec<u8>> = content
                .split_inclusive(|b| *b == b'\n')
                .map(|line| line.to_vec())
                .collect();
            let had_newline = updated_lines[idx].ends_with(b"\n");
            let had_bom = idx == 0 && updated_lines[idx].starts_with(b"\xef\xbb\xbf");
            updated_lines[idx] = Self::render_config_file_line(managed_path, had_newline);
            if had_bom {
                updated_lines[idx].splice(..0, [0xef, 0xbb, 0xbf]);
            }
            return updated_lines.concat();
        }

        // Otherwise, append the config-file line
        let config_file_line = Self::render_config_file_line(managed_path, true);
        if content.ends_with(b"\n") {
            [content, config_file_line.as_slice()].concat()
        } else {
            [content, b"\n", config_file_line.as_slice()].concat()
        }
    }

    /// Apply font-only update to Ghostty without triggering full theme reapply.
    /// Writes only font.conf and ensures it's included.
    /// Reloads Ghostty so the font change is visible immediately.
    pub fn apply_font_only(env: &SlateEnv, font_name: &str) -> Result<()> {
        let font_conf_content = super::font_config::ghostty(font_name)?;
        let config_manager = ConfigManager::with_env(env)?;

        // Write only the font-family to managed font.conf
        config_manager.write_managed_file("ghostty", "font.conf", &font_conf_content)?;

        // Ensure integration file includes the font.conf file
        let adapter = GhosttyAdapter;
        let integration_path = adapter.integration_config_path_with_env(env)?;
        if integration_path.exists() {
            adapter.strip_managed_references_from_non_primary_configs(env, &integration_path)?;
            let managed_font_path = config_manager.managed_dir("ghostty").join("font.conf");
            Self::ensure_integration_includes_managed(&integration_path, &managed_font_path)?;
        }

        // Reload Ghostty so the font change takes effect immediately
        let _ = adapter.reload_with_env(env);

        Ok(())
    }

    /// Reload Ghostty config via its own AppleScript API.
    /// macOS Automation authorization may be required; failure is surfaced
    /// without retrying through another automation mechanism.
    #[cfg(target_os = "macos")]
    fn reload_via_applescript() -> Result<()> {
        let script = r#"tell application "Ghostty"
    set target_terminal to focused terminal of selected tab of front window
    perform action "reload_config" on target_terminal
end tell"#;

        run_reload(
            Command::new("osascript").arg("-e").arg(script),
            RELOAD_LIMITS,
        )
    }

    #[cfg(not(target_os = "macos"))]
    fn send_reload_signal() -> Result<()> {
        run_reload(
            Command::new("pkill")
                .arg("-SIGUSR2")
                .arg("-x")
                .arg("ghostty"),
            RELOAD_LIMITS,
        )
    }
}

impl ToolAdapter for GhosttyAdapter {
    fn tool_name(&self) -> &'static str {
        "ghostty"
    }

    fn is_installed(&self) -> Result<bool> {
        Ok(detection::detect_tool_presence(self.tool_name()).installed)
    }

    fn is_installed_with_env(&self, env: &SlateEnv) -> Result<bool> {
        Ok(detection::detect_tool_presence_with_env(self.tool_name(), env).installed)
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

    /// preview-path override. Resolves both the integration config
    /// path and the managed config path via the injected `env` instead of
    /// `SlateEnv::from_process()`, so `silent_preview_apply(&tempdir_env, …)`
    /// lands writes inside the test sandbox rather than the user's real
    /// `~/.config/ghostty` / `~/.config/slate/managed/ghostty`.
    fn apply_theme_with_env(&self, theme: &ThemeVariant, env: &SlateEnv) -> Result<ApplyOutcome> {
        let integration_path = self.integration_config_path_with_env(env)?;
        if !integration_path.exists() {
            return Ok(ApplyOutcome::Skipped(SkipReason::MissingIntegrationConfig));
        }

        // Write palette colors directly — no Ghostty built-in theme names.
        // This ensures terminal colors exactly match our palette (used by
        // starship, bat, etc.), eliminating cross-tool color drift.
        let p = &theme.palette;
        let window_theme = Self::window_theme_for(theme.appearance);
        // Match light/dark colors without choosing a native window layout.
        // A managed titlebar-style value loads after user preferences (including
        // nested config-files), silently overriding tabs/hidden/native choices.
        // Omission also removes earlier Slate overrides when this file regenerates.
        let managed_content = format!(
            "theme =\n\
             window-theme = {window_theme}\n\
             background = {bg}\n\
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
            window_theme = window_theme,
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
        let current_font = config_manager.get_current_font()?;
        let font_conf_content = current_font
            .as_deref()
            .map(super::font_config::ghostty)
            .transpose()?;
        config_manager.write_managed_file("ghostty", "theme.conf", &managed_content)?;

        let current_opacity = config_manager.get_current_opacity_preset()?;
        write_opacity_config(env, current_opacity)?;
        write_blur_radius(env, current_opacity)?;

        if let Some(ref font_conf_content) = font_conf_content {
            config_manager.write_managed_file("ghostty", "font.conf", font_conf_content)?;
        }

        // Step 4: Ensure integration file includes all managed paths idempotently
        let managed_base = self.managed_config_path_with_env(Some(env));
        let theme_path = managed_base.join("theme.conf");
        let opacity_path = managed_base.join("opacity.conf");
        let blur_path = managed_base.join("blur.conf");

        let mut managed_paths = vec![theme_path, opacity_path, blur_path];
        if current_font.is_some() {
            managed_paths.push(managed_base.join("font.conf"));
        }

        // Ghostty 1.3.x can see both the current XDG config and the legacy
        // macOS App Support config. Keeping Slate's config-file lines in more
        // than one entry file makes Ghostty report false "cycle detected"
        // errors for the shared managed files. Rebuild Slate-owned references
        // from scratch in one write to the selected entry while preserving
        // every non-Slate line in every candidate config.
        self.rebuild_managed_references_in_selected_config(env, &integration_path, &managed_paths)?;

        // Font updates are handled by the FontAdapter.
        // Theme switches should only affect colors, not fonts.
        // Font changes are an orthogonal concern managed separately.

        // Ghostty hot-reloads the current window via its AppleScript API;
        // the theme change is visible without spawning a new shell.
        Ok(ApplyOutcome::applied_no_shell())
    }

    fn reload(&self) -> Result<()> {
        self.reload_with_env(&SlateEnv::from_process()?)
    }

    fn reload_with_env(&self, env: &SlateEnv) -> Result<()> {
        let session = env.session();
        if session.is_isolated() {
            return Ok(());
        }
        if session.is_remote() {
            return Err(crate::error::SlateError::ReloadFailed(
                "ghostty".into(),
                "SSH cannot reload the client terminal".into(),
            ));
        }
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

/// Inherent helper methods that take an explicit `SlateEnv`. Kept separate
/// from the trait impl so other modules (`apply_font_only`, tests) can call
/// them by name — trait impl blocks don't allow `pub` on individual methods.
impl GhosttyAdapter {
    fn window_theme_for(appearance: ThemeAppearance) -> &'static str {
        match appearance {
            ThemeAppearance::Dark => "dark",
            ThemeAppearance::Light => "light",
        }
    }

    pub fn integration_config_path_with_env(&self, env: &SlateEnv) -> Result<PathBuf> {
        let candidates = self.integration_candidate_paths_with_env(env)?;

        if let Some(path) = Self::last_existing_path(&candidates) {
            return Ok(path);
        }

        // Keep Slate's established zero-config destination on both platforms.
        Ok(Self::default_config_path(
            &env.xdg_config_home().join("ghostty"),
        ))
    }

    pub fn integration_candidate_paths_with_env(&self, env: &SlateEnv) -> Result<Vec<PathBuf>> {
        Ok(Self::config_candidates_with_env(env)?
            .into_iter()
            .map(|entry| entry.path)
            .collect())
    }

    /// Candidates in Ghostty 1.3.1 default-file order, with stable recovery keys.
    /// Slate writes to the last existing candidate. This is not an effective
    /// runtime loader: native preferred-file checks and process overrides differ.
    pub(crate) fn config_candidates_with_env(
        env: &SlateEnv,
    ) -> Result<Vec<paths::ConfigCandidate>> {
        let xdg_dir = env.xdg_config_home().join("ghostty");
        Ok(paths::candidates(&xdg_dir, Some(env.home())))
    }

    pub(crate) fn xdg_config_candidates(env: &SlateEnv) -> Vec<paths::ConfigCandidate> {
        paths::candidates(&env.xdg_config_home().join("ghostty"), None)
    }

    pub fn strip_managed_references_from_path(
        env: &SlateEnv,
        integration_path: &Path,
    ) -> Result<()> {
        if !integration_path.exists() {
            return Ok(());
        }

        let managed_prefix = env
            .config_dir()
            .join("managed")
            .join("ghostty")
            .to_string_lossy()
            .to_string()
            .into_bytes();
        let content = fs::read(integration_path)?;
        let cleaned = Self::strip_managed_references_from_bytes(&content, &managed_prefix);

        if cleaned != content {
            crate::config::preview_write::write_legacy(integration_path, &cleaned)?;
        }
        Ok(())
    }

    fn rebuild_managed_references_in_selected_config(
        &self,
        env: &SlateEnv,
        selected_path: &Path,
        managed_paths: &[PathBuf],
    ) -> Result<()> {
        let managed_prefix = env
            .config_dir()
            .join("managed")
            .join("ghostty")
            .to_string_lossy()
            .to_string()
            .into_bytes();
        let mut writes = Vec::new();

        for candidate in self.integration_candidate_paths_with_env(env)? {
            if !candidate.exists() {
                continue;
            }

            let content = fs::read(&candidate)?;
            let mut rebuilt = Self::strip_managed_references_from_bytes(&content, &managed_prefix);
            if candidate == selected_path {
                Self::append_managed_references(&mut rebuilt, managed_paths);
            }

            if rebuilt != content {
                writes.push((candidate, rebuilt));
            }
        }

        // Clean stale refs from non-primary entries before adding the complete
        // selected-entry ref set. If a cleanup write fails, we avoid creating
        // a fresh duplicate-ref cycle; if the selected write runs, it lands the
        // whole Slate ref set in one pass.
        writes.sort_by_key(|(path, _)| if path == selected_path { 1 } else { 0 });
        for (path, content) in writes {
            crate::config::preview_write::write_legacy(&path, &content)?;
        }

        Ok(())
    }

    pub(crate) fn strip_managed_references_from_bytes(
        content: &[u8],
        managed_prefix: &[u8],
    ) -> Vec<u8> {
        let Ok(root) = std::str::from_utf8(managed_prefix) else {
            return content.to_vec();
        };
        let mut cleaned: Vec<u8> = Vec::with_capacity(content.len());
        for (index, (line, directive)) in references::literal_lines(content).enumerate() {
            if matches!(directive, references::Directive::Path { value, .. } if references::managed(value, Path::new(root)).is_some())
            {
                if index == 0 && line.starts_with(b"\xef\xbb\xbf") {
                    cleaned.extend_from_slice(b"\xef\xbb\xbf");
                }
                continue;
            }
            cleaned.extend_from_slice(line);
        }
        cleaned
    }

    fn append_managed_references(content: &mut Vec<u8>, managed_paths: &[PathBuf]) {
        if !content.is_empty() && !content.ends_with(b"\n") {
            content.push(b'\n');
        }
        for managed_path in managed_paths {
            content.extend(Self::render_config_file_line(managed_path, true));
        }
    }

    fn strip_managed_references_from_non_primary_configs(
        &self,
        env: &SlateEnv,
        primary_path: &Path,
    ) -> Result<()> {
        for candidate in self.integration_candidate_paths_with_env(env)? {
            if candidate != primary_path {
                Self::strip_managed_references_from_path(env, &candidate)?;
            }
        }
        Ok(())
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
    crate::opacity::ManagedFile::GhosttyOpacity.write(env, opacity)
}

/// Write blur radius configuration to managed Ghostty config file.
/// Frosted preset → 20px blur, others → 0 (no blur).
/// Path: ~/.config/slate/managed/ghostty/blur.conf
pub fn write_blur_radius(env: &SlateEnv, opacity: crate::opacity::OpacityPreset) -> Result<()> {
    crate::opacity::ManagedFile::GhosttyBlur.write(env, opacity)
}

#[cfg(test)]
mod tests {
    #[test]
    fn ghostty_reload_capture_bounds_helpers_and_retains_uncertainty() {
        use crate::platform::process_output::Limits;
        use std::{process::Command, time::Duration};
        for (script, expected) in [
            ("printf 'PRIVATE\\033[2J' >&2; exit 7", "exit status: 7"),
            ("while :; do :; done", "deadline"),
            ("while :; do printf PRIVATE_OUTPUT; done", "output limit"),
        ] {
            let mut command = Command::new("/bin/sh");
            command.env_clear().args(["-c", script]);
            let error = super::run_reload(
                &mut command,
                Limits {
                    timeout: Duration::from_millis(200),
                    max_output: 1024,
                },
            )
            .unwrap_err()
            .to_string();
            assert!(error.contains(expected), "{error}");
            assert!(error.contains("may already have applied"));
            assert!(!error.contains("PRIVATE"));
            assert!(!error.contains('\x1b'));
        }
        let mut success = Command::new("/bin/sh");
        success
            .env_clear()
            .args(["-c", "read value && exit 9; exit 0"]);
        super::run_reload(&mut success, super::RELOAD_LIMITS).unwrap();
        let home = tempfile::tempdir().unwrap();
        let mut missing = Command::new(home.path().join("missing-helper"));
        assert!(super::run_reload(&mut missing, super::RELOAD_LIMITS)
            .unwrap_err()
            .to_string()
            .contains("Could not start or observe"));
    }

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
        assert!(path.ends_with("ghostty/config.ghostty"));
    }

    #[test]
    fn test_ghostty_candidate_paths_includes_both_names() {
        let env = SlateEnv::with_home(PathBuf::from("/test"));
        let candidates = GhosttyAdapter
            .integration_candidate_paths_with_env(&env)
            .unwrap();
        // Must include both config.ghostty (primary) and config (legacy)
        assert!(candidates[0].ends_with("ghostty/config"));
        assert!(candidates[1].ends_with("ghostty/config.ghostty"));
    }

    #[test]
    fn test_ghostty_last_existing_path() {
        let candidates = vec![
            PathBuf::from("/nonexistent/path1"),
            PathBuf::from("/nonexistent/path2"),
        ];
        assert!(GhosttyAdapter::last_existing_path(&candidates).is_none());
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
        assert!(path.ends_with("ghostty/config.ghostty"));
    }

    #[test]
    fn test_ghostty_integration_config_path_prefers_last_loaded_existing_config() {
        let tempdir = tempfile::tempdir().unwrap();
        let env = SlateEnv::with_home(tempdir.path().to_path_buf());
        let adapter = GhosttyAdapter;
        let ghostty_dir = env.xdg_config_home().join("ghostty");
        fs::create_dir_all(&ghostty_dir).unwrap();
        fs::write(ghostty_dir.join("config.ghostty"), "# current default\n").unwrap();
        fs::write(ghostty_dir.join("config"), "# legacy no-extension\n").unwrap();

        let path = adapter.integration_config_path_with_env(&env).unwrap();

        assert!(path.ends_with("ghostty/config.ghostty"));
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

    #[test]
    fn ensure_integration_ignores_similar_keys_and_non_exact_paths() {
        use tempfile::TempDir;

        let tempdir = TempDir::new().unwrap();
        let temp_path = tempdir.path().join("config");
        let managed_path = PathBuf::from("/home/user/.config/slate/managed/ghostty/theme.conf");
        fs::write(
            &temp_path,
            format!(
                "config-file-note = \"{}\"\ninclude-note = \"{}\"\nconfig-file = \"{}-old\"\nconfig-file = \"/tmp{}\"\ninclude = \"{}/child\"\n",
                managed_path.display(),
                managed_path.display(),
                managed_path.display(),
                managed_path.display(),
                managed_path.display(),
            ),
        )
        .unwrap();

        GhosttyAdapter::ensure_integration_includes_managed(&temp_path, &managed_path).unwrap();

        let content = fs::read_to_string(&temp_path).unwrap();
        assert!(content.contains("config-file-note ="));
        assert!(content.contains("include-note ="));
        assert!(content.contains(&format!("config-file = \"{}-old\"", managed_path.display())));
        assert!(content.contains(&format!("config-file = \"/tmp{}\"", managed_path.display())));
        assert!(content.contains(&format!("include = \"{}/child\"", managed_path.display())));
        assert_eq!(
            content
                .lines()
                .filter(|line| *line == format!("config-file = \"{}\"", managed_path.display()))
                .count(),
            1
        );
    }

    #[test]
    fn strip_managed_references_removes_config_file_and_legacy_include_only() {
        use tempfile::TempDir;

        let tempdir = TempDir::new().unwrap();
        let env = SlateEnv::with_home(tempdir.path().to_path_buf());
        let temp_path = tempdir.path().join("config");
        let managed = env.config_dir().join("managed/ghostty");
        fs::write(
            &temp_path,
            format!(
                "config-file = \"{}/theme.conf\"\ninclude = \"{}/opacity.conf\"\nconfig-file-note = \"{}/not-managed.conf\"\ninclude-note = \"{}/also-not-managed.conf\"\nconfig-file = \"{}-old/theme.conf\"\nconfig-file = \"/tmp{}/theme.conf\"\nfont-family = Menlo\n",
                managed.display(),
                managed.display(),
                managed.display(),
                managed.display(),
                managed.display(),
                managed.display(),
            ),
        )
        .unwrap();

        GhosttyAdapter::strip_managed_references_from_path(&env, &temp_path).unwrap();

        let content = fs::read_to_string(&temp_path).unwrap();
        assert!(!content.contains(&format!(
            "config-file = \"{}/theme.conf\"",
            managed.display()
        )));
        assert!(!content.contains(&format!("include = \"{}/opacity.conf\"", managed.display())));
        assert!(content.contains("config-file-note ="));
        assert!(content.contains("include-note ="));
        assert!(content.contains("managed/ghostty-old/theme.conf"));
        assert!(content.contains(&format!("/tmp{}/theme.conf", managed.display())));
        assert!(content.contains("font-family = Menlo"));
    }

    #[test]
    fn rebuild_managed_references_writes_complete_refs_to_selected_entry() {
        use tempfile::TempDir;

        let tempdir = TempDir::new().unwrap();
        let env = SlateEnv::with_home(tempdir.path().to_path_buf());
        let adapter = GhosttyAdapter;
        let ghostty_dir = env.xdg_config_home().join("ghostty");
        fs::create_dir_all(&ghostty_dir).unwrap();

        let first_entry = ghostty_dir.join("config.ghostty");
        let selected_entry = ghostty_dir.join("config");
        let managed = env.config_dir().join("managed/ghostty");
        let managed_paths = ["theme.conf", "opacity.conf", "blur.conf", "font.conf"]
            .iter()
            .map(|name| managed.join(name))
            .collect::<Vec<_>>();

        fs::write(
            &first_entry,
            format!(
                "config-file = \"{}\"\nuser-first = true\n",
                managed.join("theme.conf").display()
            ),
        )
        .unwrap();
        fs::write(
            &selected_entry,
            format!(
                "user-selected = true\nconfig-file = \"{}\"\n",
                managed.join("theme.conf").display()
            ),
        )
        .unwrap();

        adapter
            .rebuild_managed_references_in_selected_config(&env, &selected_entry, &managed_paths)
            .unwrap();

        let first_content = fs::read_to_string(&first_entry).unwrap();
        assert!(first_content.contains("user-first = true"));
        assert!(!first_content.contains("managed/ghostty"));

        let selected_content = fs::read_to_string(&selected_entry).unwrap();
        assert!(selected_content.contains("user-selected = true"));
        assert_eq!(selected_content.matches("config-file = ").count(), 4);
        for path in managed_paths {
            assert!(selected_content.contains(&path.display().to_string()));
        }
    }

    #[cfg(unix)]
    #[test]
    fn rebuild_managed_references_preserves_symlinked_entry_files() {
        use std::os::unix::fs::symlink;
        use tempfile::TempDir;

        let tempdir = TempDir::new().unwrap();
        let env = SlateEnv::with_home(tempdir.path().to_path_buf());
        let adapter = GhosttyAdapter;
        let ghostty_dir = env.xdg_config_home().join("ghostty");
        let dotfiles_dir = tempdir.path().join("dotfiles/ghostty");
        fs::create_dir_all(&ghostty_dir).unwrap();
        fs::create_dir_all(&dotfiles_dir).unwrap();

        let linked_entry = ghostty_dir.join("config.ghostty");
        let dotfile_entry = dotfiles_dir.join("config.ghostty");
        let managed = env.config_dir().join("managed/ghostty");
        let managed_paths = [managed.join("theme.conf"), managed.join("opacity.conf")];

        fs::write(&dotfile_entry, "user-symlinked = true\n").unwrap();
        symlink(&dotfile_entry, &linked_entry).unwrap();

        adapter
            .rebuild_managed_references_in_selected_config(&env, &linked_entry, &managed_paths)
            .unwrap();

        assert!(
            fs::symlink_metadata(&linked_entry)
                .unwrap()
                .file_type()
                .is_symlink(),
            "Ghostty entry path must remain a symlink for dotfile-managed configs"
        );
        let content = fs::read_to_string(&dotfile_entry).unwrap();
        assert!(content.contains("user-symlinked = true"));
        assert_eq!(content.matches("config-file = ").count(), 2);
        for path in managed_paths {
            assert!(content.contains(&path.display().to_string()));
        }
    }

    #[test]
    fn test_ensure_integration_preserves_non_utf8_prefix_bytes() {
        use tempfile::TempDir;

        let tempdir = TempDir::new().unwrap();
        let temp_path = tempdir.path().join("config");
        let managed_path = PathBuf::from("/home/user/.config/slate/managed/ghostty/theme.conf");

        fs::write(&temp_path, [0xff, b'\n']).unwrap();

        GhosttyAdapter::ensure_integration_includes_managed(&temp_path, &managed_path).unwrap();

        let content = fs::read(&temp_path).unwrap();
        assert!(content.starts_with(&[0xff, b'\n']));
        assert!(
            content
                .windows(b"config-file = ".len())
                .any(|w| w == b"config-file = "),
            "managed config-file line must still be appended"
        );
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

    #[test]
    fn apply_theme_rebuilds_slate_refs_in_single_entry_file_to_avoid_cycles() {
        let tempdir = tempfile::tempdir().unwrap();
        let env = SlateEnv::with_home(tempdir.path().to_path_buf());
        let adapter = GhosttyAdapter;
        let ghostty_dir = env.xdg_config_home().join("ghostty");
        fs::create_dir_all(&ghostty_dir).unwrap();

        let legacy_entry = ghostty_dir.join("config");
        let selected_existing = ghostty_dir.join("config.ghostty");
        let managed = env.config_dir().join("managed/ghostty");
        fs::write(
            &legacy_entry,
            format!(
                "config-file = \"{}/theme.conf\"\nuser-legacy = true\n",
                managed.display()
            ),
        )
        .unwrap();
        fs::write(&selected_existing, "user-selected-existing = true\n").unwrap();

        let theme = crate::theme::catppuccin::catppuccin_mocha().unwrap();
        adapter.apply_theme_with_env(&theme, &env).unwrap();

        let legacy_content = fs::read_to_string(&legacy_entry).unwrap();
        assert!(legacy_content.contains("user-legacy = true"));
        assert!(!legacy_content.contains("managed/ghostty"));

        let selected_content = fs::read_to_string(&selected_existing).unwrap();
        assert!(selected_content.contains("user-selected-existing = true"));
        assert_eq!(selected_content.matches("config-file = ").count(), 3);
        assert!(selected_content.contains("theme.conf"));
        assert!(selected_content.contains("opacity.conf"));
        assert!(selected_content.contains("blur.conf"));
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

    /// contract: the trait-level `apply_theme_with_env` must honor
    /// the injected env — writes MUST land inside the tempdir, and the
    /// host's real `~/.config/slate/managed/ghostty` MUST NOT be touched.
    /// This is the behavior proof that closes the 19-08 "signature lie".
    #[test]
    fn apply_theme_with_env_honors_injected_env_for_managed_writes() {
        use crate::adapter::ToolAdapter;
        use std::io::Write;
        use tempfile::TempDir;

        let tempdir = TempDir::new().unwrap();
        let env = SlateEnv::with_home(tempdir.path().to_path_buf());
        let adapter = GhosttyAdapter;

        // Pre-create the integration config file inside the tempdir so the
        // apply path doesn't early-return with MissingIntegrationConfig.
        let integration_path = adapter.integration_config_path_with_env(&env).unwrap();
        fs::create_dir_all(integration_path.parent().unwrap()).unwrap();
        let mut file = fs::File::create(&integration_path).unwrap();
        writeln!(file, "# slate managed").unwrap();
        drop(file);

        let theme = crate::theme::catppuccin::catppuccin_mocha().unwrap();

        // Route through the TRAIT method — this is what ToolRegistry dispatches
        // to when `apply_theme_to_tools_with_env` is used (Task 01).
        let outcome = ToolAdapter::apply_theme_with_env(&adapter, &theme, &env).unwrap();
        assert!(matches!(outcome, ApplyOutcome::Applied { .. }));

        // Managed writes MUST have landed inside the tempdir.
        let managed_theme = tempdir
            .path()
            .join(".config/slate/managed/ghostty/theme.conf");
        assert!(
            managed_theme.exists(),
            "expected managed theme.conf inside tempdir at {:?}",
            managed_theme
        );
        let managed_content = fs::read_to_string(&managed_theme).unwrap();
        assert!(
            managed_content.starts_with("theme =\n"),
            "managed theme.conf must reset any Ghostty built-in theme loaded from higher-priority macOS config files"
        );
        assert!(
            managed_content.contains("window-theme = dark\n"),
            "managed theme.conf must pin Ghostty window chrome to the Slate theme appearance"
        );
        assert!(
            !managed_content.contains("macos-titlebar-style"),
            "theme colors must not override the user's window layout on any platform"
        );

        // Integration file inside the tempdir must reference the managed path.
        let integration_content = fs::read_to_string(&integration_path).unwrap();
        assert!(
            integration_content.contains(&managed_theme.display().to_string()),
            "integration config must include the managed theme.conf under tempdir, got:\n{}",
            integration_content
        );
    }

    #[test]
    fn apply_theme_updates_window_appearance_without_choosing_titlebar_layout() {
        use tempfile::TempDir;

        let tempdir = TempDir::new().unwrap();
        let env = SlateEnv::with_home(tempdir.path().to_path_buf());
        let adapter = GhosttyAdapter;

        let integration_path = adapter.integration_config_path_with_env(&env).unwrap();
        fs::create_dir_all(integration_path.parent().unwrap()).unwrap();
        fs::write(&integration_path, "# slate managed\n").unwrap();

        let dark = crate::theme::catppuccin::catppuccin_mocha().unwrap();
        adapter.apply_theme_with_env(&dark, &env).unwrap();
        let managed_theme = tempdir
            .path()
            .join(".config/slate/managed/ghostty/theme.conf");
        let dark_content = fs::read_to_string(&managed_theme).unwrap();
        assert!(dark_content.contains("window-theme = dark\n"));

        let light = crate::theme::catppuccin::catppuccin_latte().unwrap();
        adapter.apply_theme_with_env(&light, &env).unwrap();
        let light_content = fs::read_to_string(&managed_theme).unwrap();
        assert!(light_content.contains("window-theme = light\n"));

        assert!(!dark_content.contains("macos-titlebar-style"));
        assert!(!light_content.contains("macos-titlebar-style"));
    }
}
