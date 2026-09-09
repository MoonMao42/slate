use crate::{
    adapter::font::PreparedFont,
    config::{recovery_paths, snapshot_font_targets_with_env},
    env::SlateEnv,
    error::{Result, SlateError},
};

pub(super) struct FontCommit {
    prepared: PreparedFont,
    recovery_id: Option<String>,
}

impl FontCommit {
    /// Called before catalog installation. Imports already own a wider point.
    pub fn prepare(env: &SlateEnv, family: &str, snapshot: bool) -> Result<Self> {
        let prepared = PreparedFont::capture(env, family)?;
        let recovery_id = if snapshot && prepared.changed() {
            let targets = recovery_paths::targets(env, prepared.paths(), "Font")?;
            let point = snapshot_font_targets_with_env(env, &targets)?;
            eprintln!("Pre-font recovery point: {}", point.id);
            eprintln!(
                "Inspect file recovery: slate restore {} --dry-run",
                point.id
            );
            eprintln!("Font recovery covers captured configuration files, not font installations, caches, empty directories or live windows.");
            Some(point.id)
        } else {
            None
        };
        prepared.verify()?;
        Ok(Self {
            prepared,
            recovery_id,
        })
    }
    pub fn apply(self) -> Result<()> {
        self.prepared.apply().map_err(|error| {
            let recovery = self.recovery_id.map(|id| format!(" Inspect file recovery: slate restore {id} --dry-run.")).unwrap_or_default();
            SlateError::InvalidConfig(format!("Font application was incomplete: {error}. The saved choice is published last; inspect its current value and any earlier file writes, which were not automatically rolled back.{recovery} Font installations and external caches are not undone."))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn font_commit_failure_reports_existing_recovery_and_preserves_external_edits() {
        let temp = tempfile::tempdir().unwrap();
        let env = SlateEnv::with_home(temp.path().to_owned());
        let commit = FontCommit::prepare(&env, "Private Mono", true).unwrap();
        let id = commit.recovery_id.clone().unwrap();
        std::fs::create_dir_all(env.config_dir()).unwrap();
        std::fs::write(env.managed_file("current-font"), "External Mono").unwrap();
        let error = commit.apply().unwrap_err().to_string();
        assert!(
            error.contains(&format!("slate restore {id} --dry-run")),
            "{error}"
        );
        assert!(error.contains("not automatically rolled back"));
        assert_eq!(
            std::fs::read(env.managed_file("current-font")).unwrap(),
            b"External Mono"
        );
        assert!(!env.managed_file("managed/ghostty/font.conf").exists());
        assert_eq!(
            crate::config::list_restore_points_with_env(&env)
                .unwrap()
                .len(),
            1
        );
    }
}
