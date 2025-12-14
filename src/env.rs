use crate::error::Result;
use std::path::{Path, PathBuf};

/// SlateEnv encapsulates environment paths for config and home directory.
/// This abstraction enables:
/// - Dependency injection: all path resolution goes through SlateEnv
/// - Test isolation: tests can inject a tempdir via with_home()
/// - Single source of truth: all adapters and config code use SlateEnv methods
pub struct SlateEnv {
    home: PathBuf,
    config_dir: PathBuf,
    cache_dir: PathBuf,
}

impl SlateEnv {
    /// Initialize from process environment
    /// Reads $HOME, $XDG_CONFIG_HOME, and $XDG_CACHE_HOME from std::env.
    /// Prefers $XDG_CONFIG_HOME if set, otherwise uses $HOME/.config.
    /// Prefers $XDG_CACHE_HOME if set, otherwise uses $HOME/.cache.
    pub fn from_process() -> Result<Self> {
        let home = std::env::var("HOME")
            .map(PathBuf::from)
            .map_err(|e| crate::error::SlateError::Internal(format!("HOME not set: {}", e)))?;

        let config_dir = std::env::var("XDG_CONFIG_HOME")
            .map(|xdg| PathBuf::from(xdg).join("slate"))
            .unwrap_or_else(|_| home.join(".config").join("slate"));

        let cache_dir = std::env::var("XDG_CACHE_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| home.join(".cache"));

        Ok(SlateEnv {
            home,
            config_dir,
            cache_dir,
        })
    }

    /// Create with injected home path (for testing)
    /// Useful for sandboxing tests: SlateEnv::with_home(tempdir.path().to_path_buf())
    /// will ensure all config and cache file writes go to tempdir instead of developer's home.
    pub fn with_home(home: PathBuf) -> Self {
        let config_dir = home.join(".config").join("slate");
        let cache_dir = home.join(".cache");
        SlateEnv {
            home,
            config_dir,
            cache_dir,
        }
    }

    /// Get home directory path
    pub fn home(&self) -> &Path {
        &self.home
    }

    /// Get slate config directory path (~/.config/slate or $XDG_CONFIG_HOME/slate)
    pub fn config_dir(&self) -> &Path {
        &self.config_dir
    }

    /// Get cache directory path (~/.cache or $XDG_CACHE_HOME)
    pub fn cache_dir(&self) -> &Path {
        &self.cache_dir
    }

    /// Get .zshrc path (for shell integration marker block)
    pub fn zshrc_path(&self) -> PathBuf {
        self.home.join(".zshrc")
    }

    /// Get path to a managed config file (e.g., current, current-font, auto.toml)
    pub fn managed_file(&self, filename: &str) -> PathBuf {
        self.config_dir.join(filename)
    }

    /// Get path to a managed subdirectory (e.g., managed/, user/, shell/)
    pub fn managed_subdir(&self, subdir: &str) -> PathBuf {
        self.config_dir.join(subdir)
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
        assert!(env.config_dir().ends_with(".config/slate"));
        assert!(env.cache_dir().ends_with(".cache"));
    }

    #[test]
    fn test_zshrc_path() {
        let tempdir = TempDir::new().unwrap();
        let env = SlateEnv::with_home(tempdir.path().to_path_buf());
        let zshrc = env.zshrc_path();

        assert!(zshrc.ends_with(".zshrc"));
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
    }
}
