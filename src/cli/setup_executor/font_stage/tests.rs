use super::*;
use crate::error::SlateError;

fn failed() -> Result<FontCacheRefresh> {
    Err(SlateError::Internal("private known failure".into()))
}

#[test]
fn font_stage_skipped_found_and_incomplete_discovery_never_install() {
    let mut skipped = ExecutionSummary::new();
    assert_eq!(
        execute_with(
            None,
            &mut skipped,
            |_| panic!("skipped selection must not scan"),
            |_, _| panic!("skipped selection must not install")
        ),
        FontCacheRefresh::NotRequested
    );
    assert!(!skipped.font_requested && !skipped.font_available && !skipped.font_applied);
    assert!(skipped.issues.is_empty());
    for found in [true, false] {
        let mut summary = ExecutionSummary::new();
        let cache = execute_with(
            Some("hack"),
            &mut summary,
            |id| {
                assert_eq!(id, "hack");
                if found {
                    Ok(true)
                } else {
                    Err(SlateError::InvalidConfig(
                        "font discovery is incomplete".into(),
                    ))
                }
            },
            |_, _| panic!("known or uncertain discovery must not install"),
        );
        assert_eq!(cache, FontCacheRefresh::NotRequested);
        assert!(summary.font_requested);
        assert_eq!(summary.font_available, found);
        assert!(!summary.font_applied);
        assert_eq!(summary.issues.len(), usize::from(!found));
        if !found {
            assert!(summary.issues[0].contains("no font download attempted"));
        }
    }
}

#[test]
fn font_stage_preserves_all_failed_attempts_in_setup_issue() {
    let mut summary = ExecutionSummary::new();
    let cache = execute_with(
        Some("hack"),
        &mut summary,
        |_| Ok(false),
        |_, progress| chain::try_install(true, failed, failed, failed, progress),
    );
    assert_eq!(cache, FontCacheRefresh::NotRequested);
    assert!(!summary.font_available && !summary.font_applied);
    assert_eq!(summary.issues.len(), 1);
    for stage in ["Homebrew:", "shared font cache:", "direct font download:"] {
        assert!(summary.issues[0].contains(stage), "{:?}", summary.issues);
    }
    assert!(!summary.is_successful());
}

#[test]
fn font_stage_cache_warning_is_success_without_repeat_installation() {
    let temp = tempfile::tempdir().unwrap();
    let marker = temp.path().join("private-font-file");
    let mut summary = ExecutionSummary::new();
    let cache = execute_with(
        Some("hack"),
        &mut summary,
        |_| Ok(false),
        |_, progress| {
            chain::try_install(
                true,
                failed,
                || {
                    std::fs::write(&marker, b"installed fixture").unwrap();
                    Ok(FontCacheRefresh::TimedOut)
                },
                || panic!("published fonts must not redownload on a cache warning"),
                progress,
            )
        },
    );
    assert_eq!(cache, FontCacheRefresh::TimedOut);
    assert!(summary.font_available && !summary.font_applied);
    assert!(summary.issues.is_empty());
    assert_eq!(summary.notices.len(), 1);
    assert!(summary.notices[0].contains("Homebrew: Internal error: private known failure"));
    assert_eq!(std::fs::read(marker).unwrap(), b"installed fixture");
    assert!(
        !summary.is_successful(),
        "availability is not saved choice or shell configuration"
    );
}

#[test]
fn font_stage_uncertain_installation_retains_partial_files_without_fallback() {
    let temp = tempfile::tempdir().unwrap();
    let marker = temp.path().join("partial-package-record");
    let mut summary = ExecutionSummary::new();
    let cache = execute_with(
        Some("hack"),
        &mut summary,
        |_| Ok(false),
        |_, progress| {
            chain::try_install(
                true,
                || {
                    std::fs::write(&marker, b"partial fixture").unwrap();
                    Err(SlateError::HomebrewInstallUncertain(
                        "private interruption".into(),
                    ))
                },
                || panic!("no shared-cache fallback"),
                || panic!("no download fallback"),
                progress,
            )
        },
    );
    assert_eq!(cache, FontCacheRefresh::NotRequested);
    assert!(!summary.font_available && !summary.font_applied);
    assert_eq!(summary.issues.len(), 1);
    assert!(summary.issues[0].contains("private interruption"));
    assert!(summary.issues[0].contains("no further font installation attempted"));
    assert_eq!(std::fs::read(marker).unwrap(), b"partial fixture");
    assert!(!summary.is_successful());
}
