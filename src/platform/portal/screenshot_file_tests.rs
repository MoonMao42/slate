use super::*;
use crate::platform::share::image_file::FIXTURE_PNG;
use std::{fs, os::unix::fs::symlink};

#[test]
fn share_image_portal_copies_borrowed_source_without_deleting_it_or_replacing_output() {
    let td = tempfile::tempdir().unwrap();
    let source = td.path().join("PRIVATE screenshot %.png");
    let target = td.path().join("saved.png");
    fs::write(&source, FIXTURE_PNG).unwrap();
    let uri = url::Url::from_file_path(&source).unwrap();
    copy_result(uri.as_str(), &target).unwrap();
    assert_eq!(fs::read(&source).unwrap(), FIXTURE_PNG);
    assert_eq!(fs::read(&target).unwrap(), FIXTURE_PNG);
    fs::write(&target, b"old output").unwrap();
    assert!(copy_result(uri.as_str(), &target).is_err());
    assert!(copy_result(uri.as_str(), &source).is_err());
    fs::hard_link(&source, td.path().join("same-inode.png")).unwrap();
    assert!(copy_result(uri.as_str(), &td.path().join("same-inode.png")).is_err());
    assert_eq!(fs::read(&target).unwrap(), b"old output");
    assert_eq!(fs::read(&source).unwrap(), FIXTURE_PNG);
}

#[test]
fn share_image_portal_invalid_uri_and_linked_sources_leave_destination_absent() {
    let td = tempfile::tempdir().unwrap();
    let source = td.path().join("PRIVATE source.png");
    fs::write(&source, FIXTURE_PNG).unwrap();
    let linked = td.path().join("link.png");
    symlink(&source, &linked).unwrap();
    let target = td.path().join("not-created/output.png");
    for uri in [
        "https://PRIVATE.example/test.png".into(),
        "PRIVATE not a URI".into(),
        "file://PRIVATE.example/source.png".into(),
        format!(
            "{}?PRIVATE=query",
            url::Url::from_file_path(&source).unwrap()
        ),
        format!("{}#PRIVATE", url::Url::from_file_path(&source).unwrap()),
        url::Url::from_file_path(linked).unwrap().to_string(),
    ] {
        let failure = copy_result(&uri, &target).unwrap_err().to_string();
        assert!(!failure.contains("PRIVATE"), "{failure}");
        assert!(!target.parent().unwrap().exists());
    }
    assert_eq!(fs::read(source).unwrap(), FIXTURE_PNG);
}
