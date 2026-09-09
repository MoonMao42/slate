//! Zellij 0.45.1 Unix directory order, captured without invoking Zellij.
use crate::error::{Result, SlateError};
use std::{
    ffi::OsString,
    path::{Path, PathBuf},
    sync::{Arc, OnceLock},
};

#[derive(Clone)]
pub(super) struct Paths {
    directory: PathBuf,
    file: PathBuf,
    candidates: Vec<PathBuf>,
    fallback: PathBuf,
    explicit_directory: bool,
    system_directory: Option<PathBuf>,
    // Shared by environment clones/parallel adapters. The first inspection
    // fixes the write set so later KDL/alias changes cannot escape a checkpoint.
    destinations: Arc<OnceLock<[(PathBuf, PathBuf); 2]>>,
}

impl Paths {
    pub(super) fn capture(
        home: &Path,
        xdg: &Path,
        vars: &impl Fn(&str) -> Option<OsString>,
        isolated: bool,
    ) -> Self {
        let fallback = home.join(".config/zellij");
        let platform = if cfg!(target_os = "macos") {
            home.join("Library/Application Support/org.Zellij-Contributors.Zellij")
        } else {
            xdg.join("zellij")
        };
        let candidates = if isolated {
            vec![fallback.clone()]
        } else {
            vec![fallback.clone(), platform, PathBuf::from("/etc/zellij")]
        };
        let override_dir = if isolated {
            None
        } else {
            vars("ZELLIJ_CONFIG_DIR").map(PathBuf::from)
        };
        let directory = override_dir
            .clone()
            .unwrap_or_else(|| select(&candidates, &fallback));
        let file = if isolated {
            None
        } else {
            vars("ZELLIJ_CONFIG_FILE").map(PathBuf::from)
        }
        .unwrap_or_else(|| directory.join("config.kdl"));
        Self {
            directory,
            file,
            candidates,
            fallback,
            explicit_directory: override_dir.is_some(),
            system_directory: (!isolated && override_dir.is_none())
                .then(|| PathBuf::from("/etc/zellij")),
            destinations: Arc::new(OnceLock::new()),
        }
    }

    pub(super) fn resolve(&self) -> Result<(&Path, &Path)> {
        let invalid = |reason| {
            SlateError::InvalidConfig(format!(
                "Zellij: {reason}; choose explicit absolute ZELLIJ_CONFIG_DIR/FILE paths"
            ))
        };
        if !self.directory.is_absolute() || !self.file.is_absolute() {
            return Err(invalid(
                "relative or empty overrides are not safe write targets",
            ));
        }
        if self.system_directory.as_ref() == Some(&self.directory) {
            return Err(invalid(
                "the implicit system configuration is not a user-owned target",
            ));
        }
        if !self.explicit_directory && select(&self.candidates, &self.fallback) != self.directory {
            return Err(invalid(
                "configuration directory precedence changed; review again",
            ));
        }
        Ok((&self.directory, &self.file))
    }

    pub(super) fn verify_destinations(&self, paths: [(&Path, &Path); 2]) -> Result<()> {
        let current = paths.map(|(path, resolved)| (path.to_owned(), resolved.to_owned()));
        let captured = self.destinations.get_or_init(|| current.clone());
        if captured != &current {
            return Err(SlateError::InvalidConfig(
                "Zellij target paths changed after inspection; reopen Slate to review the new configuration. No new target is allowed outside the original recovery set.".into(),
            ));
        }
        Ok(())
    }
}

fn select(candidates: &[PathBuf], fallback: &Path) -> PathBuf {
    candidates
        .iter()
        .find(|path| std::fs::symlink_metadata(path).is_ok())
        .cloned()
        .unwrap_or_else(|| fallback.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zellij_directory_order_rechecks_precedence_without_guessing_xdg_on_macos() {
        let home = tempfile::tempdir().unwrap();
        let xdg = home.path().join("xdg");
        let platform = if cfg!(target_os = "macos") {
            home.path()
                .join("Library/Application Support/org.Zellij-Contributors.Zellij")
        } else {
            xdg.join("zellij")
        };
        std::fs::create_dir_all(&platform).unwrap();
        let captured = Paths::capture(home.path(), &xdg, &|_| None, false);
        assert_eq!(captured.resolve().unwrap().0, platform);
        let first = home.path().join(".config/zellij");
        std::fs::create_dir_all(&first).unwrap();
        assert!(captured
            .resolve()
            .unwrap_err()
            .to_string()
            .contains("precedence changed"));
        let captured = Paths::capture(home.path(), &xdg, &|_| None, false);
        assert_eq!(
            captured.resolve().unwrap(),
            (first.as_path(), first.join("config.kdl").as_path())
        );
    }

    #[test]
    fn zellij_config_file_override_is_independent_and_isolation_ignores_both_overrides() {
        let home = tempfile::tempdir().unwrap();
        let directory = home.path().join("profile");
        let file = home.path().join("elsewhere/personal.kdl");
        let vars = |key: &str| match key {
            "ZELLIJ_CONFIG_DIR" => Some(directory.clone().into()),
            "ZELLIJ_CONFIG_FILE" => Some(file.clone().into()),
            _ => None,
        };
        let xdg = home.path().join("xdg");
        let captured = Paths::capture(home.path(), &xdg, &vars, false);
        assert_eq!(
            captured.resolve().unwrap(),
            (directory.as_path(), file.as_path())
        );
        let isolated = Paths::capture(home.path(), &xdg, &vars, true);
        assert_eq!(
            isolated.resolve().unwrap().0,
            home.path().join(".config/zellij")
        );
        assert_eq!(
            isolated.resolve().unwrap().1,
            home.path().join(".config/zellij/config.kdl")
        );
    }

    #[test]
    fn zellij_relative_empty_and_implicit_system_targets_are_rejected() {
        let home = tempfile::tempdir().unwrap();
        for key in ["ZELLIJ_CONFIG_DIR", "ZELLIJ_CONFIG_FILE"] {
            for value in ["", "relative", "~/zellij"] {
                let vars = |name: &str| (name == key).then(|| OsString::from(value));
                let captured = Paths::capture(home.path(), home.path(), &vars, false);
                assert!(captured.resolve().is_err(), "{key}={value}");
            }
        }
        // Inject the resolved system candidate without touching /etc.
        let mut system = Paths::capture(home.path(), home.path(), &|_| None, false);
        system.directory = PathBuf::from("/etc/zellij");
        system.file = system.directory.join("config.kdl");
        assert!(system
            .resolve()
            .unwrap_err()
            .to_string()
            .contains("implicit system"));
    }
}
