use super::*;
use crate::config::state_files::atomic_write_synced;
use std::os::unix::fs::{symlink, MetadataExt};

fn entry(path: &Path) -> Entry {
    let target = destination(path).unwrap();
    Entry {
        path: path.to_owned(),
        expected: read_state(&target).unwrap(),
        destination: target,
    }
}

#[test]
fn preview_receipts_follow_only_explicit_workers_and_reject_closed_contexts() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("config");
    fs::write(&path, b"ORIGINAL").unwrap();
    let scope = Scope::begin(vec![entry(&path)]).unwrap();
    let context = context().unwrap();
    std::thread::scope(|threads| {
        threads
            .spawn(|| {
                assert!(super::context().is_none());
                {
                    let _attached = context.enter();
                    {
                        let _nested = context.enter();
                        atomic_write_synced(&path, b"OWN_FIRST").unwrap();
                    }
                    atomic_write_synced(&path, b"OWN_LAST").unwrap();
                }
                assert!(super::context().is_none());
            })
            .join()
            .unwrap();
    });
    std::thread::scope(|threads| {
        threads
            .spawn(|| atomic_write_synced(&path, b"EXTERNAL").unwrap())
            .join()
            .unwrap();
    });
    let (states, failed) = scope.finish().unwrap();
    assert!(!failed);
    assert!(matches!(&states[0], FileState::Present { bytes, .. } if bytes == b"OWN_LAST"));
    assert!(super::context().is_none());
    {
        let _stale = context.enter();
        assert!(atomic_write_synced(&path, b"STALE").is_err());
    }
    assert_eq!(fs::read(&path).unwrap(), b"EXTERNAL");
    atomic_write_synced(&path, b"OUTSIDE_PREVIEW").unwrap();
}

#[test]
fn preview_receipts_recheck_content_and_writer_alias_and_preserve_legacy_links() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("config");
    let other = root.path().join("other");
    let link = root.path().join("link");
    fs::write(&path, b"ORIGINAL").unwrap();
    fs::write(&other, b"UNTOUCHED").unwrap();
    symlink(&path, &link).unwrap();
    let inode = fs::metadata(&path).unwrap().ino();
    write_legacy(&link, b"LEGACY").unwrap();
    assert_eq!(fs::metadata(&path).unwrap().ino(), inode);
    let scope = Scope::begin(vec![entry(&path)]).unwrap();
    let ticket = prepare(&link, 3).unwrap().unwrap();
    fs::remove_file(&link).unwrap();
    symlink(&other, &link).unwrap();
    assert!(ticket.verify().is_err());
    drop(ticket);
    fs::remove_file(&link).unwrap();
    symlink(&path, &link).unwrap();
    let ticket = prepare(&path, 3).unwrap().unwrap();
    fs::write(&path, b"EXTERNAL").unwrap();
    assert!(ticket.verify().is_err());
    drop(ticket);
    assert!(scope.finish().unwrap().1);
    assert_eq!(fs::read(&path).unwrap(), b"EXTERNAL");
    assert_eq!(fs::read(&other).unwrap(), b"UNTOUCHED");

    let scope = Scope::begin(vec![entry(&link), entry(&path)]).unwrap();
    write_legacy(&link, b"TRACKED").unwrap();
    let (states, failed) = scope.finish().unwrap();
    assert!(!failed);
    assert!(fs::symlink_metadata(&link)
        .unwrap()
        .file_type()
        .is_symlink());
    assert!(states[0] == states[1]);
    assert!(states[0] == read_state(&destination(&link).unwrap()).unwrap());
}

#[test]
fn preview_receipts_bound_outputs_and_reservations_before_publication() {
    let root = tempfile::tempdir().unwrap();
    let paths: Vec<_> = ["first", "second", "third"]
        .map(|name| root.path().join(name))
        .into();
    let scope = Scope::begin(paths.iter().map(|path| entry(path)).collect()).unwrap();
    let limit = MAX_TOOL_CONFIG_BYTES as usize;
    assert!(prepare(&paths[0], limit + 1).is_err());
    assert!(atomic_write_synced(&root.path().join("uncaptured"), b"NO").is_err());
    let first = prepare(&paths[0], limit).unwrap().unwrap();
    assert!(prepare(&paths[0], 1).is_err());
    let second = prepare(&paths[1], limit).unwrap().unwrap();
    assert!(prepare(&paths[2], 1).is_err());
    drop(first);
    drop(second);
    let (states, failed) = scope.finish().unwrap();
    assert!(failed, "ignored adapter errors must still be reported");
    assert!(states.iter().all(|state| *state == FileState::Absent));
    assert_eq!(fs::read_dir(root.path()).unwrap().count(), 0);

    let scope = Scope::begin(vec![entry(&paths[0])]).unwrap();
    symlink(root.path().join("missing"), &paths[0]).unwrap();
    assert!(write_legacy(&paths[0], b"NO").is_err());
    assert!(scope.finish().unwrap().1);
    assert!(!root.path().join("missing").exists());
}
