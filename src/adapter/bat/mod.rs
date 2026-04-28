//! bat adapter for theme application.
//! bat reads `BAT_THEME` from the environment (exported by slate's shell
//! integration). layered an additional pipeline on top of that:
//! every theme apply now writes a slate-tuned `.tmTheme` for ALL 20
//! registered themes into `<bat-config-dir>/themes/` and then invokes
//! `bat cache --build` (capability-gated). This keeps slate's "one
//! palette across the stack" guarantee consistent on bat output and
//! removes the dependency on bat's bundled (and stale) Sublime-derived
//! themes (sharkdp/bat issue #941).

use crate::adapter::{ApplyOutcome, ApplyStrategy, ToolAdapter};
use crate::detection;
use crate::env::SlateEnv;
use crate::error::{Result, SlateError};
use crate::theme::{ThemeRegistry, ThemeVariant};
use std::path::{Path, PathBuf};
use std::process::Command;

pub mod tmtheme;

/// bat adapter implementing the ToolAdapter trait.
pub struct BatAdapter;

impl BatAdapter {
    /// Pure path resolution: BAT_CONFIG_PATH → BAT_CONFIG_DIR/config → XDG default
    fn resolve_path(
        config_path: Option<&str>,
        config_dir: Option<&str>,
        config_home: &Path,
    ) -> PathBuf {
        if let Some(val) = config_path {
            if !val.is_empty() {
                return PathBuf::from(val);
            }
        }
        if let Some(val) = config_dir {
            if !val.is_empty() {
                return PathBuf::from(val).join("config");
            }
        }
        config_home.join("bat").join("config")
    }

    fn resolve_themes_dir(config_dir: Option<&str>, config_home: &Path) -> PathBuf {
        if let Some(val) = config_dir {
            if !val.is_empty() {
                return PathBuf::from(val).join("themes");
            }
        }
        config_home.join("bat").join("themes")
    }
}

impl ToolAdapter for BatAdapter {
    fn tool_name(&self) -> &'static str {
        "bat"
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
        ApplyStrategy::EnvironmentVariable
    }

    fn apply_theme(&self, theme: &ThemeVariant) -> Result<ApplyOutcome> {
        // Per F3: every apply syncs all 20 slate-tuned tmThemes
        // to <bat-config-dir>/themes/ and triggers `bat cache --build`.
        // BAT_THEME is still exported in shell init, so the outcome
        // remains "applied; needs new shell" for the env-var change.
        let env = SlateEnv::from_process()?;
        self.apply_theme_with_env(theme, &env)
    }

    fn apply_theme_with_env(&self, theme: &ThemeVariant, env: &SlateEnv) -> Result<ApplyOutcome> {
        BatAdapter::apply_theme_with_env(self, theme, env)
    }
}

/// Helper methods using injected SlateEnv (for testing)
impl BatAdapter {
    pub fn integration_config_path_with_env(&self, env: &SlateEnv) -> Result<PathBuf> {
        let config_home = env.xdg_config_home().to_path_buf();
        let config_path = std::env::var("BAT_CONFIG_PATH").ok();
        let config_dir = std::env::var("BAT_CONFIG_DIR").ok();

        Ok(Self::resolve_path(
            config_path.as_deref(),
            config_dir.as_deref(),
            &config_home,
        ))
    }

    pub fn managed_config_path_with_env(&self, env: Option<&SlateEnv>) -> PathBuf {
        if let Some(e) = env {
            let config_dir = e.config_dir();
            config_dir.join("managed").join("bat")
        } else {
            PathBuf::from(".config/slate/managed/bat")
        }
    }

    /// Resolve bat's custom `themes/` directory.
    /// `BAT_CONFIG_DIR` changes the config directory and therefore the custom
    /// assets directory. `BAT_CONFIG_PATH` points at a specific config file but
    /// does not change where bat looks for `themes/`, so it is intentionally
    /// ignored here.
    pub fn themes_dir(&self, env: &SlateEnv) -> PathBuf {
        let config_home = env.xdg_config_home().to_path_buf();
        let config_dir = std::env::var("BAT_CONFIG_DIR").ok();
        Self::resolve_themes_dir(config_dir.as_deref(), &config_home)
    }

    fn write_tmtheme_files(&self, themes: &[ThemeVariant], target_dir: &Path) -> Result<()> {
        std::fs::create_dir_all(target_dir).map_err(|e| {
            SlateError::ConfigWriteError(target_dir.display().to_string(), e.to_string())
        })?;

        for theme in themes {
            let xml = tmtheme::render_tmtheme(&theme.palette, &theme.id);
            let file_name = format!("slate-{}.tmTheme", theme.id);
            let file_path = target_dir.join(&file_name);

            crate::config::atomic_write_synced(&file_path, xml.as_bytes()).map_err(|e| {
                SlateError::ConfigWriteError(file_path.display().to_string(), e.to_string())
            })?;
        }

        Ok(())
    }

    /// Per-apply idempotent sync: writes ALL slate-tuned tmThemes to
    /// `target_dir`, overwriting any existing `slate-<id>.tmTheme`.
    /// Each write uses slate's shared `atomic_write_synced` helper so the
    /// parent directory is fsynced before the immediate `bat cache --build`
    /// subprocess reads it. Cost: ~160KB total atomic writes — negligible
    /// compared to the rest of `slate theme set`.
    pub fn apply_tmtheme_files(&self, themes: &[ThemeVariant], target_dir: &Path) -> Result<()> {
        self.write_tmtheme_files(themes, target_dir)?;
        let config_dir = target_dir.parent().unwrap_or_else(|| Path::new("."));
        invoke_bat_cache_rebuild(config_dir)?;
        Ok(())
    }

    /// Inject-friendly variant of `apply_theme` used by the trait dispatch
    /// and tests. Wires `apply_tmtheme_files` against the full registered
    /// theme set, then preserves the "needs new shell" outcome (BAT_THEME
    /// env var still requires a fresh shell to take effect).
    pub fn apply_theme_with_env(
        &self,
        _theme: &ThemeVariant,
        env: &SlateEnv,
    ) -> Result<ApplyOutcome> {
        let registry = ThemeRegistry::new()?;
        let all_owned: Vec<ThemeVariant> = registry.all().into_iter().cloned().collect();
        let target_dir = self.themes_dir(env);
        self.apply_tmtheme_files(&all_owned, &target_dir)?;
        Ok(ApplyOutcome::applied_needs_new_shell())
    }
}

/// Invoke `bat cache --build` if bat is on PATH. Silent no-op when bat
/// is absent (matches the existing `apply_theme` no-op behaviour for
/// missing-bat). Once bat is present, cache rebuild errors are fatal because
/// shell integration now exports `BAT_THEME=slate-<id>`.
fn invoke_bat_cache_rebuild(config_dir: &Path) -> Result<()> {
    if which::which("bat").is_err() {
        return Ok(());
    }
    match Command::new("bat")
        .env("BAT_CONFIG_DIR", config_dir.as_os_str())
        .env_remove("BAT_THEME")
        .args(["cache", "--build"])
        .output()
    {
        Ok(out) if out.status.success() => Ok(()),
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            let stdout = String::from_utf8_lossy(&out.stdout);
            let details = if stderr.trim().is_empty() {
                stdout.trim().to_string()
            } else {
                stderr.trim().to_string()
            };
            Err(SlateError::ConfigWriteError(
                "bat cache --build".to_string(),
                details,
            ))
        }
        Err(err) => {
            // which::which("bat") said yes but spawn failed (race or
            // permissions). This must be fatal because shell integration now
            // exports BAT_THEME=slate-<id>, which requires a rebuilt cache.
            Err(SlateError::ConfigWriteError(
                "bat cache --build".to_string(),
                err.to_string(),
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bat_adapter_tool_name() {
        let adapter = BatAdapter;
        assert_eq!(adapter.tool_name(), "bat");
    }

    #[test]
    fn test_bat_apply_strategy() {
        let adapter = BatAdapter;
        assert_eq!(adapter.apply_strategy(), ApplyStrategy::EnvironmentVariable);
    }

    #[test]
    fn test_bat_resolve_path_with_explicit_path() {
        let result =
            BatAdapter::resolve_path(Some("/explicit/path"), None, &PathBuf::from("/config"));
        assert_eq!(result, PathBuf::from("/explicit/path"));
    }

    #[test]
    fn test_bat_resolve_path_with_dir() {
        let result = BatAdapter::resolve_path(None, Some("/bat/dir"), &PathBuf::from("/config"));
        assert_eq!(result, PathBuf::from("/bat/dir/config"));
    }

    #[test]
    fn test_bat_resolve_path_with_default() {
        let result = BatAdapter::resolve_path(None, None, &PathBuf::from("/config"));
        assert_eq!(result, PathBuf::from("/config/bat/config"));
    }

    #[test]
    fn test_bat_resolve_themes_dir_uses_config_dir() {
        let result = BatAdapter::resolve_themes_dir(Some("/bat/dir"), &PathBuf::from("/config"));
        assert_eq!(result, PathBuf::from("/bat/dir/themes"));
    }

    #[test]
    fn test_bat_resolve_themes_dir_ignores_config_path_shape() {
        let result = BatAdapter::resolve_themes_dir(None, &PathBuf::from("/config"));
        assert_eq!(result, PathBuf::from("/config/bat/themes"));
    }

    #[test]
    fn test_bat_integration_config_path_with_env() {
        let tempdir = tempfile::tempdir().unwrap();
        let env = SlateEnv::with_home(tempdir.path().to_path_buf());
        let adapter = BatAdapter;

        let path = adapter.integration_config_path_with_env(&env).unwrap();
        assert!(path.ends_with("bat/config"));
    }

    #[test]
    fn test_bat_managed_config_path_with_env() {
        let tempdir = tempfile::tempdir().unwrap();
        let env = SlateEnv::with_home(tempdir.path().to_path_buf());
        let adapter = BatAdapter;

        let path = adapter.managed_config_path_with_env(Some(&env));
        assert!(path.ends_with("slate/managed/bat"));
    }

    /// `themes_dir` resolves to the sibling `themes/` directory of the
    /// bat integration config file. Verified against the XDG default
    /// resolution path: `<xdg_config_home>/bat/themes`.
    #[test]
    fn test_themes_dir_resolves_to_bat_themes_subdir() {
        let tempdir = tempfile::tempdir().unwrap();
        let env = SlateEnv::with_home(tempdir.path().to_path_buf());
        let adapter = BatAdapter;

        let dir = adapter.themes_dir(&env);
        assert!(
            dir.ends_with("bat/themes"),
            "expected path ending with bat/themes, got {dir:?}"
        );
    }

    /// `write_tmtheme_files` writes one `slate-<id>.tmTheme` per supplied
    /// theme into the target directory. The cache rebuild is intentionally not
    /// part of this test so a developer machine with bat installed does not
    /// rebuild the real user cache while running unit tests.
    #[test]
    fn test_write_tmtheme_files_writes_one_per_theme() {
        let tempdir = tempfile::tempdir().unwrap();
        let target_dir = tempdir.path().join("themes");

        // Use the embedded registry: pure data, no env mutation.
        let registry = ThemeRegistry::new().expect("registry loads");
        let themes: Vec<ThemeVariant> = registry.all().into_iter().take(3).cloned().collect();
        let ids: Vec<String> = themes.iter().map(|t| t.id.clone()).collect();

        let adapter = BatAdapter;
        adapter
            .write_tmtheme_files(&themes, &target_dir)
            .expect("write_tmtheme_files succeeds");

        for id in &ids {
            let file = target_dir.join(format!("slate-{id}.tmTheme"));
            assert!(file.is_file(), "expected {file:?} to exist after apply");
            let content = std::fs::read_to_string(&file).unwrap();
            assert!(content.contains("<plist version=\"1.0\">"));
            assert!(content.contains(&format!("<string>slate-{id}</string>")));
        }
    }
}
