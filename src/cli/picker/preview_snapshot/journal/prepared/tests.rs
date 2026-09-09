use super::*;

fn fixture() -> (tempfile::TempDir, SlateEnv, PathBuf) {
    let home = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(home.path().to_owned());
    let path = env.managed_file("managed/ghostty/theme.conf");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, b"PRIVATE_ORIGINAL").unwrap();
    let snapshot = PreviewSnapshot::capture(&env).unwrap();
    fs::write(&path, b"PRIVATE_PREVIEW").unwrap();
    let expected = snapshot
        .files
        .iter()
        .map(|file| read_state(&file.destination).unwrap())
        .collect();
    snapshot
        .journal
        .save(&Record::new(&snapshot, expected, false))
        .unwrap();
    drop(snapshot);
    (home, env, path)
}

#[test]
fn prepared_recovery_checks_record_and_lock_before_each_action() {
    for action in ["recover", "discard", "export"] {
        for change in ["bytes", "missing", "permissions", "lock", "parent"] {
            let (home, env, path) = fixture();
            let mut prepared = prepare_recovery(&env).unwrap().unwrap();
            assert_eq!(prepared.take_plan().unwrap().blocked_count(), 0);
            let record = record_path(&env);
            match change {
                "bytes" => fs::write(&record, b"PRIVATE_REPLACED_RECORD").unwrap(),
                "missing" => fs::remove_file(&record).unwrap(),
                "permissions" => {
                    fs::set_permissions(&record, fs::Permissions::from_mode(0o644)).unwrap()
                }
                "lock" => {
                    let next = tempfile::NamedTempFile::new_in(env.slate_cache_dir()).unwrap();
                    next.persist(crate::config::write_guard::lock_path(&env))
                        .unwrap();
                }
                "parent" => {
                    let moved = home.path().join("moved-cache");
                    fs::rename(env.slate_cache_dir(), &moved).unwrap();
                    // Same record and lock inodes, but a changed resolved parent.
                    std::os::unix::fs::symlink(&moved, env.slate_cache_dir()).unwrap();
                }
                _ => unreachable!(),
            }
            let current_record = fs::read(&record).ok();
            let export = home.path().join("export");
            let result = match action {
                "recover" => prepared.recover(),
                "discard" => prepared.discard(),
                "export" => prepared.export(&export),
                _ => unreachable!(),
            };
            let error = result.unwrap_err().to_string();
            assert!(error.contains("changed since inspection"), "{error}");
            assert!(!error.contains("PRIVATE_"));
            assert_eq!(fs::read(&path).unwrap(), b"PRIVATE_PREVIEW");
            assert_eq!(fs::read(&record).ok(), current_record);
            assert!(!export.exists());
        }
    }
}

#[test]
fn prepared_recovery_preserves_late_records_and_reports_completed_restoration() {
    let (_home, env, path) = fixture();
    let snapshot = prepare_recovery(&env)
        .unwrap()
        .unwrap()
        .into_snapshot()
        .unwrap();
    snapshot.restore_files(true).unwrap();
    // Same session, changed record after restoration but before final cleanup.
    let mut value: serde_json::Value =
        serde_json::from_slice(&fs::read(record_path(&env)).unwrap()).unwrap();
    value["writing"] = serde_json::json!(true);
    let replacement = serde_json::to_vec(&value).unwrap();
    fs::write(record_path(&env), &replacement).unwrap();
    let error = snapshot.journal.finish().unwrap_err().to_string();
    assert!(error.contains("files were restored"), "{error}");
    assert!(error.contains("not cleared") && error.contains("not rolled back"));
    assert!(!error.contains("PRIVATE_"));
    assert_eq!(fs::read(path).unwrap(), b"PRIVATE_ORIGINAL");
    assert_eq!(fs::read(record_path(&env)).unwrap(), replacement);
}

#[test]
fn prepared_recovery_oversized_discard_is_bounded_and_metadata_bound() {
    for changed in [false, true] {
        let (_home, env, path) = fixture();
        let record = record_path(&env);
        let file = OpenOptions::new().write(true).open(&record).unwrap();
        file.set_len(MAX_RECORD_BYTES + 1).unwrap();
        let source = RecordSource::capture(&env).unwrap().unwrap();
        assert!(source.parse(&env).is_err());
        let mut prepared = prepare_recovery(&env).unwrap().unwrap();
        assert!(prepared.take_plan().is_err());
        if changed {
            file.set_len(MAX_RECORD_BYTES + 2).unwrap();
        }
        assert_eq!(prepared.discard().is_err(), changed);
        assert_eq!(record.exists(), changed);
        assert_eq!(fs::read(path).unwrap(), b"PRIVATE_PREVIEW");
    }
}
