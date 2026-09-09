//! Keep the cooperative lock and captured record from inspection through action.
use super::*;

#[must_use = "Keep the prepared recovery alive through confirmation"]
pub(crate) struct PreparedRecovery {
    env: SlateEnv,
    lock: File,
    source: RecordSource,
    record: Option<Record>,
    inspection: Option<Result<RecoveryPlan>>,
}

#[cfg(test)]
mod tests;

pub(crate) fn prepare_recovery(env: &SlateEnv) -> Result<Option<PreparedRecovery>> {
    crate::config::recovery_paths::validate_file_path(
        env,
        &crate::config::write_guard::lock_path(env),
        "Cannot inspect preview recovery lock",
    )?;
    let lock = open_lock(env, false)?;
    if let Some(file) = &lock {
        if !try_lock(file)? {
            return Err(active_error());
        }
    }
    let Some(source) = RecordSource::capture(env)? else {
        return Ok(None);
    };
    let lock = lock.ok_or_else(|| invalid("Recovery record has no lock file"))?;
    record_source::verify_lock(env, &lock)?;
    let (record, inspection) = match source.parse(env) {
        Ok(record) => {
            let inspection = plan(env, Some(&record), false);
            (Some(record), Ok(inspection))
        }
        Err(err) => (None, Err(err)),
    };
    Ok(Some(PreparedRecovery {
        env: env.clone(),
        lock,
        source,
        record,
        inspection: Some(inspection),
    }))
}

impl PreparedRecovery {
    pub(crate) fn take_plan(&mut self) -> Result<RecoveryPlan> {
        self.inspection
            .take()
            .ok_or_else(|| invalid("Recovery plan was already consumed"))?
    }

    fn verify(&self) -> Result<()> {
        record_source::verify_lock(&self.env, &self.lock)?;
        self.source.verify(&self.env)
    }

    pub(crate) fn recover(self) -> Result<()> {
        self.into_snapshot()?.restore()
    }

    fn into_snapshot(mut self) -> Result<PreviewSnapshot> {
        self.verify()?;
        let record = self.record.take().ok_or_else(|| {
            invalid("Recovery record cannot be restored; inspect it before proceeding")
        })?;
        // Target files may have changed while the user was reading the plan.
        if plan(&self.env, Some(&record), false).blocked_count() > 0 {
            return Err(invalid("Recovery has conflicts. No files were restored; inspect `slate recover --dry-run` before proceeding."));
        }
        self.verify()?;
        let journal = JournalGuard {
            env: self.env.clone(),
            lock: Mutex::new(Some(self.lock)),
            session_id: record.session_id.clone(),
            finished: AtomicBool::new(false),
            expected_record: Some(self.source),
        };
        Ok(PreviewSnapshot {
            env: self.env,
            files: record.files,
            missing_dirs: record.missing_dirs,
            expected: Mutex::new(record.expected),
            operation: Mutex::new(()),
            writing: AtomicBool::new(false),
            restored: AtomicBool::new(false),
            journal,
        })
    }

    pub(crate) fn discard(self) -> Result<()> {
        // Corrupt records are allowed, but never trust their embedded paths.
        self.verify()?;
        cleanup::after_discard(cleanup::remove_record(&self.env))
    }

    /// Export captured originals to a new private directory, retaining recovery.
    pub(crate) fn export(self, directory: &Path) -> Result<()> {
        self.verify()?;
        let record = self.record.ok_or_else(|| {
            invalid("Recovery record cannot be exported; inspect it before proceeding")
        })?;
        fs::DirBuilder::new().mode(0o700).create(directory)?;
        let mut entries = Vec::new();
        for (index, file) in record.files.iter().enumerate() {
            let (original_file, mode) = match &file.original {
                FileState::Absent => (None, None),
                FileState::Present { bytes, mode } => {
                    let name = format!("{index:02}.original");
                    write_private_new(&directory.join(&name), bytes)?;
                    (Some(name), Some(mode & 0o777))
                }
            };
            entries.push(serde_json::json!({"path": file.path, "original_file": original_file, "original_mode": mode}));
        }
        write_private_new(
            &directory.join("manifest.json"),
            &serde_json::to_vec_pretty(&entries)?,
        )?;
        File::open(directory)?.sync_all()?;
        Ok(())
    }
}
