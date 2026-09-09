//! bat adapter for theme application.
//! bat reads `BAT_THEME` from the environment (exported by slate's shell
//! integration). layered an additional pipeline on top of that:
//! every theme apply now writes a slate-tuned `.tmTheme` for ALL 20
//! registered themes into `<bat-config-dir>/themes/` and then invokes
//! `bat cache --build` (capability-gated). This keeps slate's "one
//! palette across the stack" guarantee consistent on bat output and
//! removes the dependency on bat's bundled (and stale) Sublime-derived
//! themes (sharkdp/bat issue #941).

use crate::adapter::{ApplyOutcome, ApplyStrategy, SkipReason, ToolAdapter};
use crate::detection;
use crate::env::SlateEnv;
use crate::error::{Result, SlateError};
use crate::theme::{ThemeRegistry, ThemeVariant};
use std::path::{Path, PathBuf};

mod cache;
pub mod tmtheme;

/// bat adapter implementing the ToolAdapter trait.
pub struct BatAdapter;

impl ToolAdapter for BatAdapter {
    fn tool_name(&self) -> &'static str {
        "bat"
    }

    fn is_installed(&self) -> Result<bool> {
        self.is_installed_with_env(&SlateEnv::from_process()?)
    }

    fn is_installed_with_env(&self, env: &SlateEnv) -> Result<bool> {
        Ok(detected_binary(env).is_some())
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
        env.validate_bat_paths()?;
        Ok(env.bat_config_path().to_owned())
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
        env.bat_config_dir().join("themes")
    }

    fn write_tmtheme_files(&self, themes: &[ThemeVariant], target_dir: &Path) -> Result<()> {
        // This helper is also reachable through a public API accepting caller
        // supplied themes. Reject path separators before any asset is written.
        if themes.iter().any(|theme| {
            theme.id.is_empty()
                || !theme
                    .id
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        }) {
            return Err(SlateError::InvalidConfig(
                "bat theme IDs must contain only ASCII letters, digits, '-' or '_'".into(),
            ));
        }
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
    /// `target_dir` must be named `themes`, directly under the desired assets
    /// root. Uses the captured process profile's cache directory. Missing bat
    /// and invalid paths fail before writes; cache failures may leave assets.
    pub fn apply_tmtheme_files(&self, themes: &[ThemeVariant], target_dir: &Path) -> Result<()> {
        let env = SlateEnv::from_process()?;
        let binary = detected_binary(&env).ok_or_else(|| {
            SlateError::ConfigWriteError(
                "bat cache --build".into(),
                "No bat or batcat executable found; no theme files were written".into(),
            )
        })?;
        self.sync_tmtheme_files(themes, target_dir, &env, &binary)
    }

    fn sync_tmtheme_files(
        &self,
        themes: &[ThemeVariant],
        target_dir: &Path,
        env: &SlateEnv,
        binary: &Path,
    ) -> Result<()> {
        env.validate_bat_paths()?;
        let target_dir = std::path::absolute(target_dir)?;
        if target_dir.file_name() != Some(std::ffi::OsStr::new("themes")) {
            return Err(SlateError::InvalidConfig(
                "bat assets must be written to a directory named 'themes'".into(),
            ));
        }
        let config_dir = target_dir
            .parent()
            .expect("absolute themes directory has a parent");
        // Freeze the exact executable and child environment before asset writes.
        let mut build = cache::CacheBuild::prepare(binary, config_dir, env)?;
        self.write_tmtheme_files(themes, &target_dir)?;
        build.run(cache::BUILD_LIMITS)
    }

    /// Inject-friendly variant of `apply_theme` used by the trait dispatch
    /// and tests. Syncs the full registered
    /// theme set, then preserves the "needs new shell" outcome (BAT_THEME
    /// env var still requires a fresh shell to take effect).
    pub fn apply_theme_with_env(
        &self,
        _theme: &ThemeVariant,
        env: &SlateEnv,
    ) -> Result<ApplyOutcome> {
        self.apply_with_binary(env, detected_binary(env))
    }

    fn apply_with_binary(&self, env: &SlateEnv, binary: Option<PathBuf>) -> Result<ApplyOutcome> {
        let Some(binary) = binary else {
            return Ok(ApplyOutcome::Skipped(SkipReason::NotInstalled));
        };
        env.validate_bat_paths()?;
        let registry = ThemeRegistry::new()?;
        let all_owned: Vec<ThemeVariant> = registry.all().into_iter().cloned().collect();
        let target_dir = self.themes_dir(env);
        self.sync_tmtheme_files(&all_owned, &target_dir, env, &binary)?;
        Ok(ApplyOutcome::applied_needs_new_shell())
    }
}

fn detected_binary(env: &SlateEnv) -> Option<PathBuf> {
    match detection::detect_tool_presence_with_env("bat", env).evidence {
        Some(detection::ToolEvidence::Executable(binary)) => Some(binary),
        _ => None,
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
    fn bat_missing_binary_skips_before_creating_any_files() {
        let td = tempfile::tempdir().unwrap();
        let env = SlateEnv::with_home(td.path().to_owned());
        assert_eq!(
            BatAdapter.apply_with_binary(&env, None).unwrap(),
            ApplyOutcome::Skipped(SkipReason::NotInstalled)
        );
        assert_eq!(std::fs::read_dir(td.path()).unwrap().count(), 0);
    }

    #[test]
    fn bat_invalid_asset_requests_fail_before_writes_or_launch() {
        let td = tempfile::tempdir().unwrap();
        let env = SlateEnv::with_home(td.path().to_owned());
        let registry = ThemeRegistry::new().unwrap();
        let mut theme = registry.all()[0].clone();
        let binary = td.path().join("not-launched");
        let error = BatAdapter
            .sync_tmtheme_files(
                &[theme.clone()],
                &td.path().join("wrong-name"),
                &env,
                &binary,
            )
            .unwrap_err();
        assert!(error.to_string().contains("directory named 'themes'"));
        for id in [
            "",
            "../escape",
            "name/../../escape",
            "bad\0name",
            "bad\nname",
        ] {
            theme.id = id.into();
            let error = BatAdapter
                .sync_tmtheme_files(&[theme.clone()], &td.path().join("themes"), &env, &binary)
                .unwrap_err();
            assert!(error.to_string().contains("bat theme IDs"));
        }
        assert_eq!(std::fs::read_dir(td.path()).unwrap().count(), 0);
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
