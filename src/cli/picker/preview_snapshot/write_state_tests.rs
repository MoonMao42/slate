//! Drive the real state-capture/cleanup coordinator with private injected writes.
use super::*;

fn write(path: &Path, bytes: &[u8]) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    crate::config::state_files::atomic_write_synced_mode(path, bytes, Some(0o600)).unwrap();
}

fn fixture() -> (tempfile::TempDir, SlateEnv, PreviewSnapshot, [PathBuf; 3]) {
    let home = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(home.path().to_owned());
    let paths = [
        env.managed_file("managed/ghostty/theme.conf"),
        env.managed_file("managed/ghostty/font.conf"),
        env.managed_file("managed/ghostty/opacity.conf"),
    ];
    for path in &paths {
        write(path, b"PRIVATE_ORIGINAL");
    }
    let snapshot = PreviewSnapshot::capture(&env).unwrap();
    snapshot
        .apply_with(|| {
            for path in &paths {
                write(path, b"PRIVATE_RECORDED_PREVIEW");
            }
            Ok(())
        })
        .unwrap();
    (home, env, snapshot, paths)
}

#[test]
fn preview_write_state_failed_after_read_preserves_external_edits_and_known_cleanup() {
    let (_home, env, snapshot, paths) = fixture();
    let oversized = crate::config::file_read::MAX_TOOL_CONFIG_BYTES + 1;
    assert!(snapshot
        .apply_with(|| {
            fs::OpenOptions::new()
                .write(true)
                .open(&paths[1])?
                .set_len(oversized)?;
            std::thread::scope(|scope| {
                scope
                    .spawn(|| write(&paths[2], b"PRIVATE_EXTERNAL_EDIT"))
                    .join()
                    .unwrap();
            });
            Err(SlateError::Internal(
                "injected preview adapter failure".into(),
            ))
        })
        .is_err());
    let record = crate::config::write_guard::record_path(&env);
    let saved = fs::read(&record).unwrap();
    let value: serde_json::Value = serde_json::from_slice(&saved).unwrap();
    assert_eq!(value["writing"], true);
    let called = std::cell::Cell::new(false);
    assert!(snapshot
        .apply_with(|| {
            called.set(true);
            Ok(())
        })
        .is_err());
    assert!(
        !called.get(),
        "unrecorded state allowed another preview write"
    );
    assert!(snapshot.restore().is_err());
    assert_eq!(fs::read(&paths[2]).unwrap(), b"PRIVATE_EXTERNAL_EDIT");
    assert_eq!(fs::metadata(&paths[1]).unwrap().len(), oversized);
    assert_eq!(fs::read(&paths[0]).unwrap(), b"PRIVATE_ORIGINAL");
    assert_eq!(fs::read(&record).unwrap(), saved);
}

#[test]
fn preview_write_state_does_not_adopt_readable_external_edits_as_its_own_writes() {
    for timing in ["unwritten", "before_write", "after_write"] {
        let (_home, env, snapshot, paths) = fixture();
        let result = snapshot.apply_with(|| {
            write(&paths[0], b"PRIVATE_INTENDED_PREVIEW");
            if timing == "after_write" {
                write(&paths[2], b"PRIVATE_INTENDED_PREVIEW");
            }
            std::thread::scope(|scope| {
                scope
                    .spawn(|| write(&paths[2], b"PRIVATE_EXTERNAL_EDIT"))
                    .join()
                    .unwrap();
            });
            if timing == "before_write" {
                // Even an adapter swallowing this error must not claim success.
                assert!(crate::config::state_files::atomic_write_synced(
                    &paths[2],
                    b"PRIVATE_INTENDED_PREVIEW"
                )
                .is_err());
            }
            Ok(())
        });
        assert!(
            result.is_err(),
            "unrelated readable edit was adopted as a preview write"
        );
        let record = crate::config::write_guard::record_path(&env);
        let saved = fs::read(&record).unwrap();
        let value: serde_json::Value = serde_json::from_slice(&saved).unwrap();
        assert_eq!(value["writing"], true);
        let index = snapshot
            .files
            .iter()
            .position(|file| file.path == paths[2])
            .unwrap();
        let expected: FileState = serde_json::from_value(value["expected"][index].clone()).unwrap();
        let intended: &[u8] = if timing == "after_write" {
            b"PRIVATE_INTENDED_PREVIEW"
        } else {
            b"PRIVATE_RECORDED_PREVIEW"
        };
        assert!(matches!(expected, FileState::Present { bytes, .. } if bytes == intended));
        assert!(snapshot.restore().is_err());
        assert_eq!(fs::read(&paths[0]).unwrap(), b"PRIVATE_ORIGINAL");
        assert_eq!(fs::read(&paths[2]).unwrap(), b"PRIVATE_EXTERNAL_EDIT");
        assert_eq!(fs::read(&record).unwrap(), saved);
    }
}

#[test]
fn preview_write_state_unavailable_expectations_never_disable_conflict_checks() {
    let (_home, env, snapshot, paths) = fixture();
    write(&paths[0], b"PRIVATE_EXTERNAL_EDIT");
    let record = crate::config::write_guard::record_path(&env);
    let saved = fs::read(&record).unwrap();
    let held = snapshot.expected.lock().unwrap();
    assert!(snapshot.restore().is_err());
    assert_eq!(fs::read(&paths[0]).unwrap(), b"PRIVATE_EXTERNAL_EDIT");
    assert_eq!(fs::read(&paths[1]).unwrap(), b"PRIVATE_RECORDED_PREVIEW");
    assert_eq!(fs::read(&record).unwrap(), saved);
    drop(held);
    snapshot.expected.lock().unwrap().pop();
    assert!(snapshot.restore().is_err());
    let called = std::cell::Cell::new(false);
    assert!(snapshot
        .apply_with(|| {
            called.set(true);
            Ok(())
        })
        .is_err());
    assert!(!called.get());
    assert_eq!(fs::read(&paths[0]).unwrap(), b"PRIVATE_EXTERNAL_EDIT");
    assert_eq!(fs::read(&record).unwrap(), saved);
}

#[test]
fn preview_write_state_excludes_overlapping_operations_and_restores_recorded_failures() {
    for adapter_fails in [false, true] {
        let (_home, env, snapshot, paths) = fixture();
        let record = crate::config::write_guard::record_path(&env);
        let result = snapshot.apply_with(|| {
            write(&paths[0], b"PRIVATE_NEW_PREVIEW");
            let saved = fs::read(&record).unwrap();
            let error = std::thread::scope(|scope| {
                scope
                    .spawn(|| snapshot.restore())
                    .join()
                    .unwrap()
                    .unwrap_err()
            });
            assert!(error.to_string().contains("operation is still active"));
            assert!(snapshot.restore_for_commit().is_err());
            let called = std::cell::Cell::new(false);
            assert!(snapshot
                .apply_with(|| {
                    called.set(true);
                    Ok(())
                })
                .is_err());
            assert!(!called.get());
            assert_eq!(fs::read(&paths[0]).unwrap(), b"PRIVATE_NEW_PREVIEW");
            assert_eq!(fs::read(&record).unwrap(), saved);
            if adapter_fails {
                Err(SlateError::Internal(
                    "injected adapter failure with readable state".into(),
                ))
            } else {
                Ok(())
            }
        });
        assert_eq!(result.is_err(), adapter_fails);
        assert!(!snapshot.writing.load(Ordering::SeqCst));
        snapshot.restore().unwrap();
        assert!(!record.exists());
        for path in paths {
            assert_eq!(fs::read(path).unwrap(), b"PRIVATE_ORIGINAL");
        }
    }
}
