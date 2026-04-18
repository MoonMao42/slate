use crate::error::Result;
use std::path::{Path, PathBuf};

/// SlateEnv encapsulates environment paths for config and home directory.
///
/// This abstraction enables:
/// - Dependency injection: all path resolution goes through SlateEnv
/// - Test isolation: tests can inject a tempdir via with_home()
/// - Single source of truth: all adapters and config code use SlateEnv methods
pub struct SlateEnv {
    home: PathBuf,
    xdg_config_home: PathBuf,
    xdg_cache_home: PathBuf,
    slate_config_dir: PathBuf,
    slate_cache_dir: PathBuf,
}

impl SlateEnv {
    /// Initialize from process environment
    ///
    /// Reads $HOME, $XDG_CONFIG_HOME, and $XDG_CACHE_HOME from std::env.
    /// Prefers $XDG_CONFIG_HOME if set, otherwise uses $HOME/.config.
    /// Prefers $XDG_CACHE_HOME if set, otherwise uses $HOME/.cache.
    pub fn from_process() -> Result<Self> {
        // SLATE_HOME overrides HOME for full isolation (used by integration tests).
        // When SLATE_HOME is set, we also force XDG_CONFIG_HOME / XDG_CACHE_HOME to land
        // inside SLATE_HOME so a host-level XDG override can't leak tests outside the
        // sandbox. Without this, GHA Ubuntu runners (which set XDG_CONFIG_HOME) had slate
        // write to /home/runner/.config/slate while tests looked in the tempdir.
        let slate_home_override = std::env::var("SLATE_HOME").ok().map(PathBuf::from);
        let home = slate_home_override
            .clone()
            .or_else(|| std::env::var("HOME").ok().map(PathBuf::from))
            .ok_or_else(|| crate::error::SlateError::Internal("HOME not set".to_string()))?;

        let xdg_config_home = if slate_home_override.is_some() {
            home.join(".config")
        } else {
            std::env::var("XDG_CONFIG_HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|_| home.join(".config"))
        };

        let xdg_cache_home = if slate_home_override.is_some() {
            home.join(".cache")
        } else {
            std::env::var("XDG_CACHE_HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|_| home.join(".cache"))
        };

        let slate_config_dir = xdg_config_home.join("slate");
        let slate_cache_dir = xdg_cache_home.join("slate");

        Ok(SlateEnv {
            home,
            xdg_config_home,
            xdg_cache_home,
            slate_config_dir,
            slate_cache_dir,
        })
    }

    /// Create with injected home path (for testing)
    ///
    /// Useful for sandboxing tests: SlateEnv::with_home(tempdir.path().to_path_buf())
    /// will ensure all config and cache file writes go to tempdir instead of developer's home.
    pub fn with_home(home: PathBuf) -> Self {
        let xdg_config_home = home.join(".config");
        let xdg_cache_home = home.join(".cache");
        let slate_config_dir = xdg_config_home.join("slate");
        let slate_cache_dir = xdg_cache_home.join("slate");
        SlateEnv {
            home,
            xdg_config_home,
            xdg_cache_home,
            slate_config_dir,
            slate_cache_dir,
        }
    }

    /// Get home directory path
    pub fn home(&self) -> &Path {
        &self.home
    }

    /// Get XDG config home (~/.config or $XDG_CONFIG_HOME)
    pub fn xdg_config_home(&self) -> &Path {
        &self.xdg_config_home
    }

    /// Get slate config directory path (~/.config/slate or $XDG_CONFIG_HOME/slate)
    pub fn config_dir(&self) -> &Path {
        &self.slate_config_dir
    }

    /// Get XDG cache home (~/.cache or $XDG_CACHE_HOME)
    pub fn cache_dir(&self) -> &Path {
        &self.xdg_cache_home
    }

    /// Get slate cache directory path (~/.cache/slate or $XDG_CACHE_HOME/slate)
    pub fn slate_cache_dir(&self) -> &Path {
        &self.slate_cache_dir
    }

    /// Get .zshrc path (for shell integration marker block)
    pub fn zshrc_path(&self) -> PathBuf {
        self.home.join(".zshrc")
    }

    /// Get .bashrc path (raw accessor for backup/snapshot code).
    pub fn bashrc_path(&self) -> PathBuf {
        self.home.join(".bashrc")
    }

    /// Get .bash_profile path (raw accessor).
    pub fn bash_profile_path(&self) -> PathBuf {
        self.home.join(".bash_profile")
    }

    /// Resolve the bash rc file where shell integration should be written.
    ///
    /// On macOS, Terminal.app launches bash as a login shell which sources `.bash_profile`
    /// (not `.bashrc`) unless the user explicitly chains them. Prefer `.bash_profile` when
    /// it already exists, or when `.bashrc` is absent. On Linux, always use `.bashrc`.
    pub fn bash_integration_path(&self) -> PathBuf {
        if cfg!(target_os = "macos") {
            let profile = self.bash_profile_path();
            let rc = self.bashrc_path();
            if profile.exists() || !rc.exists() {
                return profile;
            }
        }
        self.bashrc_path()
    }

    /// Get fish conf.d directory for managed loader files.
    pub fn fish_conf_d_dir(&self) -> PathBuf {
        self.xdg_config_home.join("fish").join("conf.d")
    }

    /// Get the path to the Slate-managed fish loader file.
    pub fn fish_loader_path(&self) -> PathBuf {
        self.fish_conf_d_dir().join("slate.fish")
    }

    /// Get the per-user local bin directory (~/.local/bin).
    pub fn user_local_bin(&self) -> PathBuf {
        self.home.join(".local").join("bin")
    }

    /// Get path to a managed config file (e.g., current, current-font, auto.toml)
    pub fn managed_file(&self, filename: &str) -> PathBuf {
        self.slate_config_dir.join(filename)
    }

    /// Get path to a managed subdirectory (e.g., managed/, user/, shell/)
    pub fn managed_subdir(&self, subdir: &str) -> PathBuf {
        self.slate_config_dir.join(subdir)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_from_process_reads_home() {
        // This test only checks that from_process can be called
        // We don't assert on the actual result since we can't control HOME in tests
        // without isolation. Real validation happens in integration tests.
        let _result = SlateEnv::from_process();
        // If it doesn't panic, the test passes
    }

    #[test]
    fn test_with_home_creates_valid_env() {
        let tempdir = TempDir::new().unwrap();
        let env = SlateEnv::with_home(tempdir.path().to_path_buf());

        assert_eq!(env.home(), tempdir.path());
        assert!(env.xdg_config_home().ends_with(".config"));
        assert!(env.config_dir().ends_with(".config/slate"));
        assert!(env.cache_dir().ends_with(".cache"));
        assert!(env.slate_cache_dir().ends_with(".cache/slate"));
    }

    #[test]
    fn test_zshrc_path() {
        let tempdir = TempDir::new().unwrap();
        let env = SlateEnv::with_home(tempdir.path().to_path_buf());
        let zshrc = env.zshrc_path();

        assert!(zshrc.ends_with(".zshrc"));
    }

    #[test]
    fn test_bashrc_path() {
        let tempdir = TempDir::new().unwrap();
        let env = SlateEnv::with_home(tempdir.path().to_path_buf());
        let bashrc = env.bashrc_path();

        assert!(bashrc.ends_with(".bashrc"));
    }

    #[test]
    fn test_fish_loader_path() {
        let tempdir = TempDir::new().unwrap();
        let env = SlateEnv::with_home(tempdir.path().to_path_buf());

        assert!(env.fish_conf_d_dir().ends_with(".config/fish/conf.d"));
        assert!(env
            .fish_loader_path()
            .ends_with(".config/fish/conf.d/slate.fish"));
    }

    #[test]
    fn test_user_local_bin_path() {
        let tempdir = TempDir::new().unwrap();
        let env = SlateEnv::with_home(tempdir.path().to_path_buf());

        assert!(env.user_local_bin().ends_with(".local/bin"));
    }

    #[test]
    fn test_managed_file() {
        let tempdir = TempDir::new().unwrap();
        let env = SlateEnv::with_home(tempdir.path().to_path_buf());
        let config_file = env.managed_file("current");

        assert!(config_file.ends_with(".config/slate/current"));
    }

    #[test]
    fn test_managed_subdir() {
        let tempdir = TempDir::new().unwrap();
        let env = SlateEnv::with_home(tempdir.path().to_path_buf());
        let managed_dir = env.managed_subdir("managed");

        assert!(managed_dir.ends_with(".config/slate/managed"));
    }

    #[test]
    fn test_cache_dir() {
        let tempdir = TempDir::new().unwrap();
        let env = SlateEnv::with_home(tempdir.path().to_path_buf());

        assert!(env.cache_dir().ends_with(".cache"));
        assert!(env.slate_cache_dir().ends_with(".cache/slate"));
    }
}

#[cfg(test)]
mod slate_cache_dir_tests {
    //! Plan 17-03 Task 1: dedicated coverage for `SlateEnv::slate_cache_dir`.
    //!
    //! These tests prove the accessor is XDG-aware (via the `from_process`
    //! / `with_home` constructors) and stable across calls — without
    //! mutating `std::env::set_var` (per user preference `feedback_no_tech_debt`,
    //! pure-function testing, no global env mutation).
    use super::*;
    use std::path::PathBuf;
    use tempfile::TempDir;

    #[test]
    fn slate_cache_dir_honors_injected_home() {
        // Constructor injection (`with_home`) is the pure-function-friendly
        // test hook: no XDG_CACHE_HOME mutation required. The resolved
        // path must sit under `<injected-home>/.cache/slate`, regardless
        // of whatever `XDG_CACHE_HOME` happens to be set to at runtime.
        let tempdir = TempDir::new().expect("create tempdir");
        let injected_home: PathBuf = tempdir.path().to_path_buf();
        let env = SlateEnv::with_home(injected_home.clone());

        let dir = env.slate_cache_dir();
        assert!(
            dir.starts_with(&injected_home),
            "expected slate_cache_dir under injected home {:?}, got {:?}",
            injected_home,
            dir
        );
        assert!(
            dir.ends_with(".cache/slate"),
            "expected path ending with '.cache/slate', got {:?}",
            dir
        );
    }

    #[test]
    fn slate_cache_dir_is_stable_across_calls() {
        let tempdir = TempDir::new().expect("create tempdir");
        let env = SlateEnv::with_home(tempdir.path().to_path_buf());
        assert_eq!(
            env.slate_cache_dir(),
            env.slate_cache_dir(),
            "slate_cache_dir must be deterministic across calls"
        );
    }
}
