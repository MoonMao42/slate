use super::*;
use image_file::FIXTURE_PNG;
use std::{
    cell::RefCell,
    fs,
    os::unix::{ffi::OsStringExt, fs::PermissionsExt},
    path::PathBuf,
};

#[test]
fn share_image_draft_cancellation_missing_and_invalid_results_never_leave_a_capture() {
    for case in ["cancel", "absent", "invalid"] {
        let temporary = RefCell::new(PathBuf::new());
        let result = draft_with(|path| {
            *temporary.borrow_mut() = path.to_owned();
            if case != "absent" {
                fs::write(path, b"PRIVATE partial output")?;
            }
            Ok(ShareCaptureResult {
                captured: case != "cancel",
                reason: Some("cancelled".into()),
            })
        });
        if case == "invalid" {
            assert!(result.is_err());
        } else {
            assert!(
                matches!(result.unwrap(), CaptureDraft::Unavailable(result) if !result.captured)
            );
        }
        assert!(!temporary.borrow().parent().unwrap().exists());
    }
}

#[test]
fn share_image_successful_draft_detaches_bytes_before_temporary_cleanup() {
    let temporary = RefCell::new(PathBuf::new());
    let draft = draft_with(|path| {
        *temporary.borrow_mut() = path.to_owned();
        fs::write(path, FIXTURE_PNG)?;
        Ok(ShareCaptureResult {
            captured: true,
            reason: None,
        })
    })
    .unwrap();
    let CaptureDraft::Captured(image) = draft else {
        panic!("expected image");
    };
    assert!(!temporary.borrow().parent().unwrap().exists());
    let td = tempfile::tempdir().unwrap();
    image.save_new(&td.path().join("saved.png")).unwrap();
    assert_eq!(fs::read(td.path().join("saved.png")).unwrap(), FIXTURE_PNG);
}

#[test]
fn share_image_native_output_path_passes_non_utf8_arguments_without_fallback() {
    let td = tempfile::tempdir().unwrap();
    let program = td.path().join("capture-fixture");
    let log = td.path().join("argument-bytes");
    // APFS need not accept an invalid UTF-8 filename. Check the argv bytes
    // at the process boundary instead of requiring filesystem support.
    fs::write(
        &program,
        format!(
            "#!/bin/sh\nfor arg in \"$@\"; do output=\"$arg\"; done\nprintf '%s' \"$output\" > {}\n",
            crate::detection::shell_quote_path(&log)
        ),
    )
    .unwrap();
    fs::set_permissions(&program, fs::Permissions::from_mode(0o755)).unwrap();
    let output = td
        .path()
        .join(std::ffi::OsString::from_vec(b"capture-\xff.png".to_vec()));
    assert!(
        native_capture(&program, &["-a", "-f"], &output, "cancelled")
            .unwrap()
            .captured
    );
    assert_eq!(
        fs::read(log).unwrap(),
        output.as_os_str().as_encoded_bytes()
    );
    assert!(!td.path().join("slate-share.png").exists());
}
