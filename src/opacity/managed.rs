//! Shared opacity rendering and bounded, identity-aware managed-file publication.
//! Captured bytes are deliberately not Debug: diagnostics must not expose them.
use super::OpacityPreset;
use crate::config::file_read::{self, Links, Source, MAX_STATE_BYTES, MAX_TOOL_CONFIG_BYTES};
use crate::config::state_files::atomic_write_synced_mode;
use crate::env::SlateEnv;
use crate::error::{Result, SlateError};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy)]
pub(crate) enum ManagedFile {
    GhosttyOpacity,
    GhosttyBlur,
    AlacrittyOpacity,
    KittyOpacity,
}

pub(crate) const MANAGED_FILES: [ManagedFile; 4] = [
    ManagedFile::GhosttyOpacity,
    ManagedFile::GhosttyBlur,
    ManagedFile::AlacrittyOpacity,
    ManagedFile::KittyOpacity,
];

impl ManagedFile {
    pub(crate) fn key(self) -> &'static str {
        match self {
            Self::GhosttyOpacity => "ghostty-opacity",
            Self::GhosttyBlur => "ghostty-blur",
            Self::AlacrittyOpacity => "alacritty-opacity",
            Self::KittyOpacity => "kitty-opacity",
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::GhosttyOpacity => "ghostty/opacity.conf",
            Self::GhosttyBlur => "ghostty/blur.conf",
            Self::AlacrittyOpacity => "alacritty/opacity.toml",
            Self::KittyOpacity => "kitty/opacity.conf",
        }
    }

    pub(crate) fn stage(self) -> &'static str {
        match self {
            Self::GhosttyOpacity => "Ghostty opacity",
            Self::GhosttyBlur => "Ghostty blur",
            Self::AlacrittyOpacity => "Alacritty opacity",
            Self::KittyOpacity => "Kitty opacity",
        }
    }

    pub(crate) fn path(self, env: &SlateEnv) -> PathBuf {
        env.config_dir().join("managed").join(self.name())
    }

    fn render(self, opacity: OpacityPreset) -> String {
        let value = opacity.to_f32();
        match self {
            Self::GhosttyOpacity => format!("background-opacity = {value}\n"),
            Self::GhosttyBlur => format!("background-blur = {}\n", opacity.blur_radius()),
            Self::AlacrittyOpacity => format!("[window]\nopacity = {value}\n"),
            Self::KittyOpacity => format!("background_opacity {value}\n"),
        }
    }

    /// Exact generated-byte comparison shared with read-only diagnostics. This
    /// does not parse user configuration or assert effective runtime opacity.
    pub(crate) fn content_matches(self, opacity: OpacityPreset, bytes: &[u8]) -> bool {
        bytes == self.render(opacity).as_bytes()
    }

    fn prepare(self, env: &SlateEnv, opacity: OpacityPreset) -> Result<PreparedWrite> {
        PreparedWrite::capture(
            self.path(env),
            self.render(opacity),
            self.stage(),
            MAX_TOOL_CONFIG_BYTES,
        )
    }

    /// Adapters share the coordinator's exact template and no-op rules. The
    /// enclosing theme/picker/import operation owns any required checkpoint.
    pub(crate) fn write(self, env: &SlateEnv, opacity: OpacityPreset) -> Result<()> {
        self.prepare(env, opacity)?.publish()
    }
}

pub(crate) fn managed_paths(env: &SlateEnv) -> impl Iterator<Item = PathBuf> + '_ {
    MANAGED_FILES.into_iter().map(|file| file.path(env))
}

pub(crate) struct PreparedOpacity {
    files: Vec<PreparedWrite>,
}

impl PreparedOpacity {
    pub(crate) fn capture(
        env: &SlateEnv,
        opacity: OpacityPreset,
        persist_state: bool,
    ) -> Result<Self> {
        let mut files = MANAGED_FILES
            .into_iter()
            .map(|file| file.prepare(env, opacity))
            .collect::<Result<Vec<_>>>()?;
        if persist_state {
            // Tracking is canonicalized on an explicit write, not compared only
            // as a parsed preset. It is always the final publication.
            files.push(PreparedWrite::capture(
                env.managed_file("current-opacity"),
                opacity.to_string().to_lowercase(),
                "saved opacity",
                MAX_STATE_BYTES,
            )?);
        }
        Ok(Self { files })
    }

    pub(crate) fn changed(&self) -> bool {
        self.files.iter().any(PreparedWrite::changed)
    }

    pub(crate) fn publish(&self) -> Result<()> {
        for file in &self.files {
            file.publish()?;
        }
        Ok(())
    }
}

struct PreparedWrite {
    path: PathBuf,
    location: PathBuf,
    original: Option<Source>,
    desired: String,
    stage: &'static str,
    limit: u64,
}

fn failure(stage: &str, path: &Path, reason: impl std::fmt::Display) -> SlateError {
    SlateError::ConfigWriteError(format!("{stage} ({})", path.display()), reason.to_string())
}

impl PreparedWrite {
    fn capture(path: PathBuf, desired: String, stage: &'static str, limit: u64) -> Result<Self> {
        let original = file_read::read(&path, limit, Links::Reject)
            .map_err(|err| failure(stage, &path, err))?;
        let location = file_read::directory_alias_target(&path)
            .ok_or_else(|| failure(stage, &path, "cannot resolve target directory"))?;
        Ok(Self {
            path,
            location,
            original,
            desired,
            stage,
            limit,
        })
    }

    fn changed(&self) -> bool {
        self.original
            .as_ref()
            .is_none_or(|source| source.bytes != self.desired.as_bytes())
    }

    fn publish(&self) -> Result<()> {
        // Recheck even unchanged files: replacement, redirected directories,
        // removed files or newly unsafe links must never become a false no-op.
        let current = file_read::read(&self.path, self.limit, Links::Reject)
            .map_err(|err| failure(self.stage, &self.path, err))?;
        if current != self.original
            || file_read::directory_alias_target(&self.path).as_ref() != Some(&self.location)
        {
            return Err(failure(
                self.stage,
                &self.path,
                "file or target directory changed after inspection; retry",
            ));
        }
        if !self.changed() {
            return Ok(());
        }
        let parent = self
            .path
            .parent()
            .ok_or_else(|| failure(self.stage, &self.path, "missing parent directory"))?;
        std::fs::create_dir_all(parent).map_err(|err| failure(self.stage, &self.path, err))?;
        let mode = self.original.as_ref().and_then(|source| source.mode);
        atomic_write_synced_mode(&self.path, self.desired.as_bytes(), mode)
            .map_err(|err| failure(self.stage, &self.path, err))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::{symlink, PermissionsExt};

    #[test]
    fn opacity_prepared_absence_never_overwrites_a_new_file() {
        let home = tempfile::tempdir().unwrap();
        let env = SlateEnv::with_home(home.path().to_owned());
        let prepared = ManagedFile::GhosttyOpacity
            .prepare(&env, OpacityPreset::Clear)
            .unwrap();
        let target = ManagedFile::GhosttyOpacity.path(&env);
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::write(&target, "PRIVATE_NEW_FILE").unwrap();
        let error = prepared.publish().unwrap_err().to_string();
        assert!(!error.contains("PRIVATE_NEW_FILE"));
        assert_eq!(fs::read_to_string(target).unwrap(), "PRIVATE_NEW_FILE");
    }

    #[test]
    fn opacity_prepared_write_rejects_late_changes_even_for_noops() {
        for changed in [false, true] {
            for mutation in [
                "edit",
                "replace",
                "mode",
                "link",
                "delete",
                "directory-alias",
            ] {
                let home = tempfile::tempdir().unwrap();
                let env = SlateEnv::with_home(home.path().to_owned());
                let target = ManagedFile::GhosttyOpacity.path(&env);
                let actual = home.path().join("actual");
                fs::create_dir(&actual).unwrap();
                fs::create_dir_all(target.parent().unwrap().parent().unwrap()).unwrap();
                symlink(&actual, target.parent().unwrap()).unwrap();
                fs::write(&target, "background-opacity = 1\n").unwrap();
                fs::set_permissions(&target, fs::Permissions::from_mode(0o600)).unwrap();
                let prepared = ManagedFile::GhosttyOpacity
                    .prepare(
                        &env,
                        if changed {
                            OpacityPreset::Clear
                        } else {
                            OpacityPreset::Solid
                        },
                    )
                    .unwrap();
                assert_eq!(prepared.changed(), changed);
                match mutation {
                    "edit" => fs::write(&target, "PRIVATE_LATE_EDIT").unwrap(),
                    "replace" => {
                        let other = home.path().join("replacement");
                        fs::write(&other, "background-opacity = 1\n").unwrap();
                        fs::set_permissions(&other, fs::Permissions::from_mode(0o600)).unwrap();
                        fs::rename(other, &target).unwrap();
                    }
                    "mode" => {
                        fs::set_permissions(&target, fs::Permissions::from_mode(0o640)).unwrap()
                    }
                    "link" => {
                        let other = home.path().join("private-link-target");
                        fs::write(&other, "PRIVATE_LATE_EDIT").unwrap();
                        fs::remove_file(&target).unwrap();
                        symlink(other, &target).unwrap();
                    }
                    "delete" => fs::remove_file(&target).unwrap(),
                    "directory-alias" => {
                        let other = home.path().join("other-dir");
                        fs::create_dir(&other).unwrap();
                        // Same bytes, mode AND inode: only the resolved path
                        // comparison can distinguish this redirected output.
                        fs::hard_link(&target, other.join("opacity.conf")).unwrap();
                        fs::remove_file(target.parent().unwrap()).unwrap();
                        symlink(other, target.parent().unwrap()).unwrap();
                    }
                    _ => unreachable!(),
                }
                let bytes = fs::read(&target).ok();
                let meta = fs::symlink_metadata(&target)
                    .ok()
                    .map(|m| m.permissions().mode());
                let error = prepared.publish().unwrap_err().to_string();
                assert!(!error.contains("PRIVATE_LATE_EDIT"), "{error}");
                assert_eq!(fs::read(&target).ok(), bytes, "{mutation}");
                assert_eq!(
                    fs::symlink_metadata(&target)
                        .ok()
                        .map(|m| m.permissions().mode()),
                    meta
                );
            }
        }
    }
}
