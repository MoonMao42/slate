use super::*;
use crate::platform::share::image_file::FIXTURE_PNG;
use std::{fs, os::unix::fs::PermissionsExt};

#[test]
fn share_image_watermark_failure_keeps_original_bytes_and_cleans_private_files() {
    let td = tempfile::tempdir().unwrap();
    let source = td.path().join("original.png");
    fs::write(&source, FIXTURE_PNG).unwrap();
    let image = CapturedImage::read(&source).unwrap().unwrap();
    let program = td.path().join("magick-fixture");
    let log = td.path().join("paths");
    for (body, expected) in [
        (
            "printf PRIVATE > \"$input\"; printf PRIVATE > \"$output\"; exit 23",
            "command failed",
        ),
        ("printf PRIVATE > \"$output\"; exit 0", "PNG signature"),
        ("exit 0", "produced no image"),
        (
            "i=0; while [ \"$i\" -lt 300 ]; do printf PRIVATE_OUTPUT; i=$((i+1)); done",
            "output limit",
        ),
        ("printf PRIVATE >&2; exec /bin/sleep 10", "timed out"),
    ] {
        fs::write(&program, format!("#!/bin/sh\ninput=${{1#png:}}\nfor arg in \"$@\"; do output=${{arg#png:}}; done\nprintf '%s\\n%s\\n' \"$input\" \"$output\" > {}\n{body}\n", crate::detection::shell_quote_path(&log))).unwrap();
        fs::set_permissions(&program, fs::Permissions::from_mode(0o755)).unwrap();
        let error = run(
            &program,
            &image,
            "slate://fixture",
            Limits {
                timeout: if expected == "timed out" {
                    Duration::from_millis(200)
                } else {
                    Duration::from_secs(2)
                },
                max_output: 1024,
            },
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains(expected), "{error}");
        assert!(!error.contains("PRIVATE"));
        for path in fs::read_to_string(&log).unwrap().lines() {
            assert!(!Path::new(path).parent().unwrap().exists());
        }
        assert_eq!(fs::read(&source).unwrap(), FIXTURE_PNG);
        let saved = image.save_unique(&td.path().join("saved.png")).unwrap();
        assert_eq!(fs::read(saved).unwrap(), FIXTURE_PNG);
    }
}

#[test]
fn share_image_watermark_success_uses_separate_output_and_preserves_original() {
    let td = tempfile::tempdir().unwrap();
    let source = td.path().join("original.png");
    fs::write(&source, FIXTURE_PNG).unwrap();
    let image = CapturedImage::read(&source).unwrap().unwrap();
    let program = td.path().join("magick-fixture");
    fs::write(&program, "#!/bin/sh\ninput=${1#png:}\nfor arg in \"$@\"; do output=${arg#png:}; done\n[ \"$input\" != \"$output\" ] || exit 31\n/bin/cp \"$input\" \"$output\"\nprintf fixture-mark >> \"$output\"\n").unwrap();
    fs::set_permissions(&program, fs::Permissions::from_mode(0o755)).unwrap();
    let marked = run(&program, &image, "slate://fixture", LIMITS).unwrap();
    marked.save_new(&td.path().join("marked.png")).unwrap();
    assert_eq!(fs::read(&source).unwrap(), FIXTURE_PNG);
    assert!(fs::read(td.path().join("marked.png"))
        .unwrap()
        .ends_with(b"fixture-mark"));
}
