//! Remember Neovim auto-activation consent per profile, not across all appnames.
use super::{file_read, ConfigManager};
use crate::error::{Result, SlateError};

const DISABLED: &[u8] = b"disabled\n";

impl ConfigManager {
    /// Whether setup may add the Neovim activation line. Missing means the
    /// default setup policy; this does not describe a running editor or hook.
    pub fn is_editor_auto_activation_enabled(&self) -> Result<bool> {
        let path = self.env.nvim_auto_activation_path();
        let source = file_read::read(&path, file_read::MAX_STATE_BYTES, file_read::Links::Reject)
            .map_err(|error| {
            SlateError::ConfigReadError(path.display().to_string(), error.to_string())
        })?;
        match source {
            None => Ok(true),
            Some(source) if source.bytes == DISABLED => Ok(false),
            Some(_) => Err(SlateError::ConfigReadError(
                path.display().to_string(),
                "invalid Neovim auto-activation preference".into(),
            )),
        }
    }

    /// Persist permission for future setup. Enabling does not insert a hook or
    /// launch Neovim; disabling callers separately remove owned activation lines.
    pub fn set_editor_auto_activation_enabled(&self, enabled: bool) -> Result<()> {
        if self.is_editor_auto_activation_enabled()? == enabled {
            return Ok(());
        }
        let path = self.env.nvim_auto_activation_path();
        if enabled {
            std::fs::remove_file(&path)?;
        } else {
            std::fs::create_dir_all(path.parent().expect("profile loader directory"))?;
            super::atomic_write_synced(&path, DISABLED)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::env::SlateEnv;

    #[test]
    fn editor_preference_is_profile_local_and_default_read_is_nonmutating() {
        let td = tempfile::tempdir().unwrap();
        let profile = |name: &str| {
            SlateEnv::from_vars(|key| match key {
                "HOME" => Some(td.path().as_os_str().to_owned()),
                "XDG_CONFIG_HOME" => Some(td.path().join("config root").into_os_string()),
                "NVIM_APPNAME" => Some(name.into()),
                _ => None,
            })
            .unwrap()
        };
        let work = profile("profiles/work");
        let other = profile("nvim");
        let config = ConfigManager::from_env_paths(&work);
        assert!(config.is_editor_auto_activation_enabled().unwrap());
        config.set_editor_auto_activation_enabled(true).unwrap();
        assert!(std::fs::read_dir(td.path()).unwrap().next().is_none());
        config.set_editor_auto_activation_enabled(false).unwrap();
        assert!(!config.is_editor_auto_activation_enabled().unwrap());
        assert!(ConfigManager::from_env_paths(&other)
            .is_editor_auto_activation_enabled()
            .unwrap());
        assert_eq!(
            std::fs::read(work.nvim_auto_activation_path()).unwrap(),
            DISABLED
        );
        config.set_editor_auto_activation_enabled(false).unwrap();
        config.set_editor_auto_activation_enabled(true).unwrap();
        assert!(config.is_editor_auto_activation_enabled().unwrap());
        assert!(!work.nvim_init_path().exists());
    }

    #[test]
    fn editor_preference_refuses_invalid_or_unsafe_existing_records() {
        for kind in ["content", "large", "directory", "link"] {
            let td = tempfile::tempdir().unwrap();
            let env = SlateEnv::with_home(td.path().to_owned());
            let config = ConfigManager::from_env_paths(&env);
            let path = env.nvim_auto_activation_path();
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            match kind {
                "content" => std::fs::write(&path, "PRIVATE_INVALID\u{1b}").unwrap(),
                "large" => std::fs::write(&path, vec![b'x'; 4097]).unwrap(),
                "directory" => std::fs::create_dir(&path).unwrap(),
                "link" => std::os::unix::fs::symlink("missing", &path).unwrap(),
                _ => unreachable!(),
            }
            let before = std::fs::symlink_metadata(&path).unwrap().file_type();
            let error = config
                .is_editor_auto_activation_enabled()
                .unwrap_err()
                .to_string();
            assert!(!error.contains("PRIVATE_INVALID"));
            for enabled in [true, false] {
                assert!(config.set_editor_auto_activation_enabled(enabled).is_err());
            }
            assert_eq!(
                std::fs::symlink_metadata(&path).unwrap().file_type(),
                before
            );
            if kind == "content" {
                assert_eq!(
                    std::fs::read_to_string(&path).unwrap(),
                    "PRIVATE_INVALID\u{1b}"
                );
            }
        }
    }
}
