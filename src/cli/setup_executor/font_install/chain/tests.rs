use super::*;

fn try_catalog_install(
    homebrew: bool,
    brew: impl FnOnce() -> Result<FontCacheRefresh>,
    caskroom: impl FnOnce() -> Result<FontCacheRefresh>,
    download: impl FnOnce() -> Result<FontCacheRefresh>,
) -> Result<FontCacheRefresh> {
    try_install(homebrew, brew, caskroom, download, |_| {}).map(|report| report.cache)
}

#[test]
fn font_brew_fallback_chain_stops_on_uncertainty_and_preserves_failed_stages() {
    use crate::{error::SlateError, platform::fonts::FontCacheRefresh};
    let uncertain = || {
        Err(SlateError::HomebrewInstallUncertain(
            "fixture interruption".into(),
        ))
    };
    let failed = || Err(SlateError::Internal("known fixture failure".into()));
    let forbidden = || -> crate::error::Result<FontCacheRefresh> {
        panic!("uncertain installation must stop automatic fallbacks")
    };
    for error in [
        try_catalog_install(true, uncertain, forbidden, forbidden).unwrap_err(),
        try_catalog_install(true, failed, uncertain, forbidden).unwrap_err(),
        try_catalog_install(true, failed, failed, uncertain).unwrap_err(),
    ] {
        assert!(matches!(error, SlateError::HomebrewInstallUncertain(_)));
    }
    let error = try_catalog_install(true, failed, failed, failed)
        .unwrap_err()
        .to_string();
    for stage in ["Homebrew:", "shared font cache:", "direct font download:"] {
        assert!(error.contains(stage), "{error}");
    }
    let error = try_catalog_install(false, forbidden, forbidden, failed)
        .unwrap_err()
        .to_string();
    assert!(!error.contains("Homebrew:") && !error.contains("shared font cache:"));
}

#[test]
fn font_cache_fallback_chain_retains_warnings_without_redownloading() {
    use crate::{error::SlateError, platform::fonts::FontCacheRefresh};
    let failed = || Err(SlateError::Internal("fixture installation failed".into()));
    for outcome in [
        FontCacheRefresh::Refreshed,
        FontCacheRefresh::Failed,
        FontCacheRefresh::MissingDependency,
        FontCacheRefresh::TimedOut,
    ] {
        assert_eq!(
            try_catalog_install(
                true,
                failed,
                || Ok(outcome),
                || panic!("cache warnings must not redownload installed fonts")
            )
            .unwrap(),
            outcome
        );
        assert_eq!(
            try_catalog_install(
                false,
                || panic!("Linux must not install via Homebrew"),
                || panic!("Linux must not use Caskroom"),
                || Ok(outcome)
            )
            .unwrap(),
            outcome
        );
        assert_eq!(
            try_catalog_install(true, failed, failed, || Ok(outcome)).unwrap(),
            outcome
        );
    }
    assert_eq!(
        try_catalog_install(
            true,
            || Ok(FontCacheRefresh::NotNeeded),
            || panic!("brew succeeded"),
            || panic!("brew succeeded")
        )
        .unwrap(),
        FontCacheRefresh::NotNeeded
    );
    assert!(try_catalog_install(false, failed, failed, failed).is_err());
}

#[test]
fn font_chain_reports_actual_source_and_recovered_failures_in_order() {
    use std::cell::RefCell;
    for success in [Stage::Homebrew, Stage::SharedCache, Stage::Download] {
        let calls = RefCell::new(Vec::new());
        let attempt = |stage: Stage| {
            calls.borrow_mut().push(stage);
            if stage == success {
                Ok(FontCacheRefresh::TimedOut)
            } else {
                Err(SlateError::Internal(format!(
                    "private {} failure",
                    stage.label()
                )))
            }
        };
        let mut progress = Vec::new();
        let report = try_install(
            true,
            || attempt(Stage::Homebrew),
            || attempt(Stage::SharedCache),
            || attempt(Stage::Download),
            |stage| progress.push(stage),
        )
        .unwrap();
        let expected = match success {
            Stage::Homebrew => vec![Stage::Homebrew],
            Stage::SharedCache => vec![Stage::Homebrew, Stage::SharedCache],
            Stage::Download => vec![Stage::Homebrew, Stage::SharedCache, Stage::Download],
        };
        assert_eq!(*calls.borrow(), expected);
        assert_eq!(progress, expected);
        assert_eq!(report.stage, success);
        assert_eq!(report.cache, FontCacheRefresh::TimedOut);
        assert_eq!(report.notices.len(), expected.len() - 1);
        for (notice, stage) in report.notices.iter().zip(&expected) {
            assert!(notice.starts_with(stage.label()), "{notice}");
        }
    }
}

#[test]
fn font_chain_unknown_source_fails_before_helpers_or_progress() {
    let temp = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(temp.path().to_owned());
    let error =
        install_catalog("Unknown\u{1b}[2J Font", &env, |_| panic!("no source")).unwrap_err();
    let message = error.to_string();
    assert!(message.contains("No automatic installation source"));
    assert!(!message.contains('\u{1b}'));
    assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 0);
}
