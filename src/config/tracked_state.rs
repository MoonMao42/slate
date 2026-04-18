use super::{state_files, ConfigManager};
use crate::error::Result;
use std::fs;

impl ConfigManager {
    /// Update current theme tracking file.
    pub fn set_current_theme(&self, theme_id: &str) -> Result<()> {
        state_files::write_state_file(&self.current_theme_path(), theme_id)
    }

    /// Get current theme ID from tracking file.
    pub fn get_current_theme(&self) -> Result<Option<String>> {
        state_files::read_optional_state_file(&self.current_theme_path())
    }

    /// Persist user's chosen font family name.
    pub fn set_current_font(&self, font_family: &str) -> Result<()> {
        state_files::write_state_file(&self.current_font_path(), font_family)
    }

    /// Get the user's chosen font family name.
    pub fn get_current_font(&self) -> Result<Option<String>> {
        state_files::read_optional_state_file(&self.current_font_path())
    }

    /// Get the current opacity preset.
    pub fn get_current_opacity(&self) -> Result<Option<String>> {
        state_files::read_optional_state_file(&self.current_opacity_path())
    }

    /// Get the current opacity preset, parsing from file.
    pub fn get_current_opacity_preset(&self) -> Result<crate::opacity::OpacityPreset> {
        self.get_current_opacity()?
            .map(|value| value.parse::<crate::opacity::OpacityPreset>())
            .transpose()?
            .map_or(Ok(crate::opacity::OpacityPreset::Solid), Ok)
    }

    /// Set the current opacity preset, persisting to file.
    pub fn set_current_opacity_preset(&self, preset: crate::opacity::OpacityPreset) -> Result<()> {
        state_files::write_state_file(
            &self.current_opacity_path(),
            &preset.to_string().to_lowercase(),
        )
    }

    /// Check if fastfetch auto-run is enabled via marker file.
    pub fn has_fastfetch_autorun(&self) -> Result<bool> {
        Ok(self.base_path.join("autorun-fastfetch").exists())
    }

    /// Enable fastfetch auto-run by creating marker file atomically.
    pub fn enable_fastfetch_autorun(&self) -> Result<()> {
        state_files::write_state_file(&self.base_path.join("autorun-fastfetch"), "")
    }

    /// Disable fastfetch auto-run by deleting marker file.
    pub fn disable_fastfetch_autorun(&self) -> Result<()> {
        let path = self.base_path.join("autorun-fastfetch");
        match fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(err) => Err(err.into()),
        }
    }

    /// Phase 16 (LS-03 / D-B3): has the user already seen the macOS BSD-`ls`
    /// capability message? Presence of the flat marker file means yes.
    ///
    /// The marker lives directly under `~/.config/slate/` (flat, matching the
    /// `autorun-fastfetch` convention) — not under a `state/` subdir.
    pub fn is_ls_capability_acknowledged(&self) -> Result<bool> {
        Ok(self.base_path.join("ls-capability-acknowledged").exists())
    }

    /// Phase 16 (LS-03 / D-B3): mark the BSD-`ls` capability message as seen.
    /// Creates `~/.config/slate/ls-capability-acknowledged` atomically. Idempotent:
    /// calling twice is fine; `slate clean` wipes the whole base_path so no
    /// disable helper is needed.
    pub fn acknowledge_ls_capability(&self) -> Result<()> {
        state_files::write_state_file(
            &self.base_path.join("ls-capability-acknowledged"),
            "",
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};
    use tempfile::TempDir;

    fn test_config_manager(base_path: &Path) -> ConfigManager {
        ConfigManager {
            base_path: base_path.to_path_buf(),
            backup_root: base_path.join(".cache/slate/backups"),
            home_path: base_path.to_path_buf(),
        }
    }

    fn acknowledged_path(base: &Path) -> PathBuf {
        base.join("ls-capability-acknowledged")
    }

    #[test]
    fn is_ls_capability_acknowledged_false_when_flag_absent() {
        let temp = TempDir::new().unwrap();
        let cm = test_config_manager(temp.path());
        assert!(!cm.is_ls_capability_acknowledged().unwrap());
    }

    #[test]
    fn acknowledge_ls_capability_creates_flat_file() {
        let temp = TempDir::new().unwrap();
        let cm = test_config_manager(temp.path());

        cm.acknowledge_ls_capability().unwrap();

        // Positive: flat marker exists directly under base_path.
        assert!(
            acknowledged_path(temp.path()).exists(),
            "flat marker file must exist at base_path/ls-capability-acknowledged"
        );
        // Negative: we did NOT create a `state/` subdirectory. The codebase uses
        // flat state files and CONTEXT D-B3's working-name reference to a sibling
        // of `state/` was corrected in RESEARCH §Pattern 3.
        assert!(
            !temp.path().join("state").exists(),
            "acknowledge must NOT create a state/ subdir"
        );
    }

    #[test]
    fn is_ls_capability_acknowledged_true_after_ack() {
        let temp = TempDir::new().unwrap();
        let cm = test_config_manager(temp.path());

        cm.acknowledge_ls_capability().unwrap();
        assert!(cm.is_ls_capability_acknowledged().unwrap());
    }

    #[test]
    fn acknowledge_ls_capability_is_idempotent() {
        let temp = TempDir::new().unwrap();
        let cm = test_config_manager(temp.path());

        cm.acknowledge_ls_capability().unwrap();
        cm.acknowledge_ls_capability().unwrap();
        // File still exists exactly once (atomic_write_file replaces in place).
        assert!(acknowledged_path(temp.path()).exists());
        assert!(cm.is_ls_capability_acknowledged().unwrap());
    }
}
