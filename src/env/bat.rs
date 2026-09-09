//! Snapshot bat's separate config file, asset root and compiled cache root.
use crate::error::{Result, SlateError};
use std::{
    ffi::OsString,
    path::{Path, PathBuf},
};

#[derive(Clone)]
pub(super) struct BatPaths {
    pub config_dir: PathBuf,
    pub config_file: PathBuf,
    pub cache_dir: PathBuf,
    error: Option<String>,
}

impl BatPaths {
    pub fn capture(
        vars: &impl Fn(&str) -> Option<OsString>,
        isolated: bool,
        config_home: &Path,
        cache_home: &Path,
    ) -> Self {
        let supplied = |name| if isolated { None } else { vars(name) };
        let mut error = None;
        let config_dir = anchor(
            supplied("BAT_CONFIG_DIR")
                .map(PathBuf::from)
                .unwrap_or_else(|| config_home.join("bat")),
            "BAT_CONFIG_DIR",
            &mut error,
        );
        let cache_dir = anchor(
            supplied("BAT_CACHE_PATH")
                .map(PathBuf::from)
                .unwrap_or_else(|| cache_home.join("bat")),
            "BAT_CACHE_PATH",
            &mut error,
        );
        let config_file = anchor(
            // bat itself reads this one with env::var (UTF-8), unlike its
            // directory overrides, which use var_os. Preserve that distinction.
            supplied("BAT_CONFIG_PATH")
                .filter(|v| v.to_str().is_some())
                .map(PathBuf::from)
                .unwrap_or_else(|| config_dir.join("config")),
            "BAT_CONFIG_PATH",
            &mut error,
        );
        Self {
            config_dir,
            config_file,
            cache_dir,
            error,
        }
    }

    pub fn validate(&self) -> Result<()> {
        match &self.error {
            Some(error) => Err(SlateError::InvalidConfig(error.clone())),
            None => Ok(()),
        }
    }
}

fn anchor(path: PathBuf, variable: &str, error: &mut Option<String>) -> PathBuf {
    if path.as_os_str().as_encoded_bytes().contains(&0) {
        error.get_or_insert_with(|| format!("{variable} contains a NUL byte"));
        return path;
    }
    // Like bat, an explicitly empty directory override denotes the current
    // directory, not the XDG default. Anchor once, before later cwd changes.
    let input = if path.as_os_str().is_empty() {
        Path::new(".")
    } else {
        &path
    };
    match std::path::absolute(input) {
        Ok(absolute) => absolute, // Preserve '..' and symlink traversal semantics.
        Err(_) => {
            error.get_or_insert_with(|| {
                format!("Cannot anchor {variable} to the current directory")
            });
            path
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::env::SlateEnv;
    use std::os::unix::ffi::OsStringExt;

    #[test]
    fn bat_paths_capture_separate_config_asset_and_cache_overrides() {
        let env = SlateEnv::from_vars(|name| match name {
            "HOME" => Some("/private-home".into()),
            "BAT_CONFIG_PATH" => Some("/config-file/batrc".into()),
            "BAT_CONFIG_DIR" => Some("/assets".into()),
            "BAT_CACHE_PATH" => Some("/compiled".into()),
            _ => None,
        })
        .unwrap();
        assert_eq!(env.bat_config_path(), Path::new("/config-file/batrc"));
        assert_eq!(env.bat_config_dir(), Path::new("/assets"));
        assert_eq!(env.bat_cache_dir(), Path::new("/compiled"));
        assert_eq!(env.clone().bat_config_dir(), env.bat_config_dir());
        env.validate_bat_paths().unwrap();
        let defaults = SlateEnv::from_vars(|name| match name {
            "HOME" => Some("/private-home".into()),
            "XDG_CONFIG_HOME" => Some("/xdg-config".into()),
            "XDG_CACHE_HOME" => Some("/xdg-cache".into()),
            "BAT_CONFIG_PATH" => Some("/separate/batrc".into()),
            _ => None,
        })
        .unwrap();
        assert_eq!(defaults.bat_config_dir(), Path::new("/xdg-config/bat"));
        assert_eq!(defaults.bat_cache_dir(), Path::new("/xdg-cache/bat"));
        assert_eq!(defaults.bat_config_path(), Path::new("/separate/batrc"));
    }

    #[test]
    fn bat_paths_isolation_ignores_even_invalid_ambient_overrides() {
        let td = tempfile::tempdir().unwrap();
        let env = SlateEnv::from_vars(|name| match name {
            "SLATE_HOME" => Some(td.path().as_os_str().to_owned()),
            "HOME" | "XDG_CONFIG_HOME" | "XDG_CACHE_HOME" | "BAT_CONFIG_PATH"
            | "BAT_CONFIG_DIR" | "BAT_CACHE_PATH" => Some("/bad\0path".into()),
            _ => None,
        })
        .unwrap();
        for env in [env, SlateEnv::with_home(td.path().to_owned())] {
            env.validate_bat_paths().unwrap();
            assert_eq!(env.bat_config_dir(), td.path().join(".config/bat"));
            assert_eq!(env.bat_config_path(), td.path().join(".config/bat/config"));
            assert_eq!(env.bat_cache_dir(), td.path().join(".cache/bat"));
        }
        assert_eq!(std::fs::read_dir(td.path()).unwrap().count(), 0);
    }

    #[test]
    fn bat_paths_anchor_relative_empty_and_non_utf8_without_filesystem_writes() {
        let cwd = std::env::current_dir().unwrap();
        let raw = OsString::from_vec(b"raw-\xff".to_vec());
        let paths = BatPaths::capture(
            &|name| match name {
                "BAT_CONFIG_DIR" => Some(raw.clone()),
                "BAT_CONFIG_PATH" => Some(raw.clone()), // bat ignores invalid UTF-8 for this variable
                "BAT_CACHE_PATH" => Some("cache/../compiled".into()),
                _ => None,
            },
            false,
            Path::new("/config"),
            Path::new("/cache"),
        );
        paths.validate().unwrap();
        assert_eq!(paths.config_dir, cwd.join(&raw));
        assert_eq!(paths.config_file, cwd.join(&raw).join("config"));
        assert_eq!(paths.cache_dir, cwd.join("cache/../compiled"));
        let empty = BatPaths::capture(
            &|_| Some(OsString::new()),
            false,
            Path::new("/config"),
            Path::new("/cache"),
        );
        assert_eq!(empty.config_dir, cwd);
        assert_eq!(empty.cache_dir, cwd);
        assert_eq!(empty.config_file, cwd);
    }

    #[test]
    fn bat_paths_invalid_override_remains_an_error_not_a_default() {
        for variable in ["BAT_CONFIG_DIR", "BAT_CONFIG_PATH", "BAT_CACHE_PATH"] {
            let paths = BatPaths::capture(
                &|name| (name == variable).then(|| "/PRIVATE\0CONTENTS".into()),
                false,
                Path::new("/config"),
                Path::new("/cache"),
            );
            let error = paths.validate().unwrap_err().to_string();
            assert!(error.contains(variable));
            assert!(!error.contains("PRIVATE"));
        }
    }
}
