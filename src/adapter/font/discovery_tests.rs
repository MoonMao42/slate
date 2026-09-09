use super::*;
use std::{
    ffi::CString,
    os::unix::{
        ffi::OsStrExt,
        fs::{symlink, PermissionsExt},
    },
};

const FONT: &[u8] = b"\0\x01\0\0private-font-discovery-fixture";

fn font(root: &Path, name: &str, bytes: &[u8]) {
    let path = root.join(name);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
}

#[test]
fn font_discovery_one_scan_finds_nested_uppercase_and_collection_candidates() {
    let temp = tempfile::tempdir().unwrap();
    font(temp.path(), "truetype/deep/ANerdFont-Regular.TTF", FONT);
    font(
        temp.path(),
        "opentype/BNerdFontMono-Bold.OTF",
        b"OTTOprivate-font-fixture",
    );
    font(
        temp.path(),
        "collection/CNerdFontPropo.TTC",
        b"ttcfprivate-collection-fixture",
    );
    font(
        temp.path(),
        "collection/DNerdFont.otC",
        b"ttcfprivate-collection-fixture",
    );
    font(
        temp.path(),
        "legacy/ENerdFont.ttf",
        b"trueprivate-legacy-fixture",
    );
    font(
        temp.path(),
        "legacy/FNerdFont.ttf",
        b"typ1private-legacy-fixture",
    );
    font(temp.path(), "truetype/dejavu/DejaVuSansMono-Bold.ttf", FONT);
    font(
        temp.path(),
        "truetype/dejavu/DejaVuSansMono-Oblique.ttf",
        FONT,
    );
    let report = scan_roots(&[temp.path().to_owned()], &["DejaVu Sans Mono"], LIMITS);
    assert!(report.is_complete(), "{}", report.warning());
    assert_eq!(
        report.fonts.nerd_fonts,
        [
            "A Nerd Font",
            "B Nerd Font Mono",
            "C Nerd Font Propo",
            "D Nerd Font",
            "E Nerd Font",
            "F Nerd Font"
        ]
    );
    assert_eq!(report.fonts.system_fonts, ["DejaVu Sans Mono"]);
    assert_eq!(
        FontAdapter::normalize_font_family("My-Real-FamilyNerdFont-BoldItalic.TTF"),
        "My-Real-Family Nerd Font"
    );
}

#[test]
fn font_discovery_rejects_directories_special_files_and_obvious_nonfonts_without_opening_them() {
    let temp = tempfile::tempdir().unwrap();
    font(temp.path(), "GoodNerdFont-Regular.ttf", FONT);
    font(
        temp.path(),
        "TextNerdFont.ttf",
        b"this is not a font, even though its filename says so",
    );
    font(temp.path(), "EmptyNerdFont.OTF", b"");
    font(temp.path(), "ShortNerdFont.TTC", b"ttcf");
    font(temp.path(), "NoteNerdFont.txt", FONT);
    font(temp.path(), "Control\nNerdFont.ttf", FONT);
    fs::create_dir(temp.path().join("DirectoryNerdFont.ttf")).unwrap();
    let fifo = temp.path().join("PipeNerdFont.ttf");
    let fifo_name = CString::new(fifo.as_os_str().as_bytes()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(fifo_name.as_ptr(), 0o600) }, 0);
    let socket_path = temp.path().join("SocketNerdFont.ttf");
    let _listener = std::os::unix::net::UnixListener::bind(socket_path).unwrap();
    let report = scan_roots(&[temp.path().to_owned()], &[], LIMITS);
    assert!(report.is_complete(), "{}", report.warning());
    assert_eq!(report.fonts.nerd_fonts, ["Good Nerd Font"]);
}

#[test]
fn font_discovery_follows_ordinary_links_and_deduplicates_directory_aliases_and_cycles() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("root");
    let linked = temp.path().join("linked");
    font(&root, "ANerdFont.ttf", FONT);
    font(&linked, "BNerdFont.ttf", FONT);
    symlink(&linked, root.join("external-font-directory")).unwrap();
    symlink(&root, linked.join("cycle-to-root")).unwrap();
    symlink(linked.join("BNerdFont.ttf"), root.join("BNerdFont.ttf")).unwrap();
    let report = scan_roots(
        &[root.clone(), root.join("external-font-directory"), linked],
        &[],
        LIMITS,
    );
    assert!(report.is_complete(), "{}", report.warning());
    assert_eq!(report.fonts.nerd_fonts, ["A Nerd Font", "B Nerd Font"]);
    assert_eq!(fs::read(root.join("BNerdFont.ttf")).unwrap(), FONT);
}

#[test]
fn font_discovery_incomplete_scans_preserve_positive_evidence_but_never_prove_absence() {
    let temp = tempfile::tempdir().unwrap();
    font(temp.path(), "ANerdFont.ttf", FONT);
    symlink(
        temp.path().join("missing"),
        temp.path().join("DanglingNerdFont.ttf"),
    )
    .unwrap();
    let report = scan_roots(&[temp.path().to_owned()], &[], LIMITS);
    assert!(!report.is_complete());
    assert!(report.contains_nerd_family("A Nerd Font").unwrap());
    assert!(report.contains_nerd_family("B Nerd Font").is_err());
    assert!(report.require_complete().is_err());
    let absent = scan_roots(&[temp.path().join("does-not-exist")], &[], LIMITS);
    assert!(absent.is_complete());
    assert!(!absent.contains_nerd_family("A Nerd Font").unwrap());
    let linked_root = temp.path().join("root-link");
    symlink(temp.path().join("missing-root"), &linked_root).unwrap();
    assert!(!scan_roots(std::slice::from_ref(&linked_root), &[], LIMITS).is_complete());
    assert!(!scan_roots(&[linked_root.join("fonts")], &[], LIMITS).is_complete());
    assert!(!scan_roots(&[temp.path().join("ANerdFont.ttf")], &[], LIMITS).is_complete());
}

#[test]
fn font_discovery_depth_entry_directory_and_issue_limits_are_explicit_and_bounded() {
    let temp = tempfile::tempdir().unwrap();
    font(temp.path(), "level/child/ANerdFont.ttf", FONT);
    for limits in [
        Limits {
            entries: 0,
            ..LIMITS
        },
        Limits {
            directories: 0,
            ..LIMITS
        },
        Limits { depth: 0, ..LIMITS },
    ] {
        let report = scan_roots(&[temp.path().to_owned()], &[], limits);
        assert!(!report.is_complete());
        assert!(report.warning().contains("limit"));
        assert!(report.contains_nerd_family("A Nerd Font").is_err());
    }
    for index in 0..MAX_ISSUES + 5 {
        symlink(
            temp.path().join("missing"),
            temp.path().join(format!("{index}NerdFont.ttf")),
        )
        .unwrap();
    }
    let report = scan_roots(&[temp.path().to_owned()], &[], LIMITS);
    assert_eq!(report.issues.len(), MAX_ISSUES);
    assert_eq!(report.omitted_issues, 5);
    assert!(report.contains_nerd_family("A Nerd Font").unwrap());
    assert!(report.warning().len() < 1200);
}

#[test]
fn font_discovery_unreadable_candidates_are_not_silently_empty_and_diagnostics_escape_paths() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("ANerdFont.ttf");
    fs::write(&path, FONT).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o000)).unwrap();
    let report = scan_roots(&[temp.path().to_owned()], &[], LIMITS);
    // Root can read mode-000 files; do not pretend this tests a denial there.
    if unsafe { libc::geteuid() } != 0 {
        assert!(!report.is_complete());
        assert!(report.fonts.nerd_fonts.is_empty());
    }
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    let mut report = FontScanReport::default();
    report.issue(
        &temp.path().join("SECRET\n\u{1b}[31m"),
        "cannot inspect font directory",
    );
    assert!(!report.warning().contains('\n') && !report.warning().contains('\u{1b}'));
    assert!(report.warning().contains("\\n"));
}
#[test]
fn font_scan_warning_languages_keep_unknown_status_and_safe_paths() {
    use crate::adapter::font::{FontScanIssue, FontScanReport};
    use crate::config::ui_language::UiLanguage::{Chinese, English};
    let report = FontScanReport {
        issues: vec![FontScanIssue {
            path: "/fixture/fonts\n\x1b[2J".into(),
            reason: "permission denied",
        }],
        omitted_issues: 2,
        ..Default::default()
    };
    let zh = report.warning_in(Chinese);
    assert!(zh.contains("3 个问题") && zh.contains("不要据此认定字体缺失"));
    assert!(zh.contains("permission denied") && zh.contains("/fixture/fonts"));
    assert!(!zh.contains('\x1b') && !zh.contains('\n'));
    assert!(!report.is_complete());
    assert_eq!(report.warning_in(English), report.warning());
}
