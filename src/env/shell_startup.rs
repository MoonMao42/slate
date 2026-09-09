//! Writable-entry selection, not evaluation of a running shell's startup logic.
use super::SlateEnv;
use std::{fs, io::ErrorKind, path::PathBuf};

impl SlateEnv {
    pub fn bash_login_path(&self) -> PathBuf {
        self.home().join(".bash_login")
    }

    pub fn shell_profile_path(&self) -> PathBuf {
        self.home().join(".profile")
    }

    /// All Bash entries Slate can edit, including the shared POSIX profile.
    pub(crate) fn bash_startup_paths(&self) -> [PathBuf; 4] {
        [
            self.bashrc_path(),
            self.bash_profile_path(),
            self.bash_login_path(),
            self.shell_profile_path(),
        ]
    }

    /// macOS targets login Bash; reuse the first present login entry to avoid
    /// shadowing it with a new .bash_profile. Linux keeps its .bashrc convention.
    /// This is conservative write selection: an unsafe/unreadable candidate is
    /// selected for validation, never treated as absent to bypass it.
    pub fn bash_integration_path(&self) -> PathBuf {
        self.bash_integration_path_for_login(cfg!(target_os = "macos"))
    }

    pub(crate) fn bash_integration_path_for_login(&self, login: bool) -> PathBuf {
        if login {
            for path in [
                self.bash_profile_path(),
                self.bash_login_path(),
                self.shell_profile_path(),
            ] {
                if !matches!(fs::symlink_metadata(&path), Err(error) if error.kind() == ErrorKind::NotFound)
                {
                    return path;
                }
            }
            self.bash_profile_path()
        } else {
            self.bashrc_path()
        }
    }
}

#[cfg(test)]
mod tests;
