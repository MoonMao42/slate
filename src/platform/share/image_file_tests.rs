use super::*;
use std::{
    os::unix::{
        ffi::OsStrExt,
        fs::{symlink, PermissionsExt},
    },
    sync::{Arc, Barrier},
    time::{Duration, Instant},
};

#[test]
fn share_image_publication_keeps_existing_files_links_and_directories() {
    let td = tempfile::tempdir().unwrap();
    let raw = td.path().join("source.png");
    fs::write(&raw, FIXTURE_PNG).unwrap();
    let image = CapturedImage::read(&raw).unwrap().unwrap();
    let base = td.path().join("slate-share.png");
    fs::write(&base, b"old capture").unwrap();
    fs::create_dir(td.path().join("slate-share-2.png")).unwrap();
    symlink(
        td.path().join("absent-target"),
        td.path().join("slate-share-3.png"),
    )
    .unwrap();
    for path in [
        &raw,
        &base,
        &td.path().join("slate-share-2.png"),
        &td.path().join("slate-share-3.png"),
    ] {
        assert!(image.save_new(path).is_err());
    }
    let saved = image.save_unique(&base).unwrap();
    assert_eq!(saved.file_name().unwrap(), "slate-share-4.png");
    assert_eq!(fs::read(&saved).unwrap(), FIXTURE_PNG);
    assert_eq!(
        fs::metadata(saved).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert_eq!(fs::read(base).unwrap(), b"old capture");
    assert_eq!(fs::read(raw).unwrap(), FIXTURE_PNG);
    assert!(!td.path().join("absent-target").exists());
    assert_eq!(
        fs::read_dir(td.path()).unwrap().count(),
        5,
        "private publication directories must be removed"
    );
}

#[test]
fn share_image_parallel_publications_choose_distinct_names_without_clobbering() {
    let td = tempfile::tempdir().unwrap();
    let raw = td.path().join("source.png");
    fs::write(&raw, FIXTURE_PNG).unwrap();
    let image = Arc::new(CapturedImage::read(&raw).unwrap().unwrap());
    let barrier = Arc::new(Barrier::new(3));
    let workers = (0..2)
        .map(|_| {
            let image = image.clone();
            let barrier = barrier.clone();
            let target = td.path().join("slate-share.png");
            std::thread::spawn(move || {
                barrier.wait();
                image.save_unique(&target).unwrap()
            })
        })
        .collect::<Vec<_>>();
    barrier.wait();
    let names = workers
        .into_iter()
        .map(|w| w.join().unwrap())
        .collect::<Vec<_>>();
    assert_ne!(names[0], names[1]);
    for name in names {
        assert_eq!(fs::read(name).unwrap(), FIXTURE_PNG);
    }
}

#[test]
fn share_image_reader_rejects_unsafe_missing_or_oversized_results_without_blocking() {
    let td = tempfile::tempdir().unwrap();
    let root = td.path();
    fs::write(root.join("good"), FIXTURE_PNG).unwrap();
    symlink(root.join("good"), root.join("link")).unwrap();
    symlink(root.join("missing"), root.join("broken")).unwrap();
    fs::create_dir(root.join("directory")).unwrap();
    fs::write(root.join("empty"), []).unwrap();
    fs::write(root.join("bad"), b"PRIVATE non-PNG result").unwrap();
    fs::write(root.join("header-only"), PNG_SIGNATURE).unwrap();
    fs::File::create(root.join("large"))
        .unwrap()
        .set_len(MAX_IMAGE_BYTES + 1)
        .unwrap();
    let fifo = root.join("fifo");
    let name = std::ffi::CString::new(fifo.as_os_str().as_bytes()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
    let started = Instant::now();
    for name in [
        "link",
        "broken",
        "directory",
        "empty",
        "bad",
        "header-only",
        "large",
        "fifo",
    ] {
        let failure = CapturedImage::read(&root.join(name))
            .unwrap_err()
            .to_string();
        assert!(!failure.contains("PRIVATE"));
    }
    assert!(CapturedImage::read(&root.join("missing"))
        .unwrap()
        .is_none());
    assert!(started.elapsed() < Duration::from_secs(2));
}
