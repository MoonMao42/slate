use super::*;
use std::{cell::Cell, fs};

fn fixture() -> (tempfile::TempDir, SlateEnv) {
    let temp = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(temp.path().to_owned());
    fs::create_dir_all(env.config_dir()).unwrap();
    fs::write(env.managed_file("current-font"), "Old Mono").unwrap();
    (temp, env)
}

#[test]
fn font_flow_preflight_failure_never_starts_readiness_or_installation() {
    let (_temp, env) = fixture();
    let entry = env.xdg_config_home().join("alacritty/alacritty.toml");
    fs::create_dir_all(entry.parent().unwrap()).unwrap();
    fs::write(&entry, "[invalid PRIVATE_CONTENT").unwrap();
    let error = apply_with(
        &ResolvedFontChoice::Catalog("Hack Nerd Font".into()),
        &env,
        true,
        || panic!("readiness preceded preflight"),
        |_| panic!("installation preceded preflight"),
    )
    .unwrap_err()
    .to_string();
    assert!(!error.contains("PRIVATE_CONTENT"));
    assert!(!env.managed_file("managed/ghostty/font.conf").exists());
    assert!(!env.slate_cache_dir().join("backups").exists());
    assert_eq!(
        fs::read(env.managed_file("current-font")).unwrap(),
        b"Old Mono"
    );
}

#[test]
// SWATCH-RENDERER: injected hostile error text verifies output escaping.
fn font_flow_install_failure_is_an_error_after_checkpoint_without_config_application() {
    let (_temp, env) = fixture();
    let ready = Cell::new(false);
    let error = apply_with(
        &ResolvedFontChoice::Catalog("Hack Nerd Font".into()),
        &env,
        true,
        || {
            assert_eq!(
                crate::config::list_restore_points_with_env(&env)
                    .unwrap()
                    .len(),
                1
            );
            ready.set(true);
        },
        |family| {
            assert!(ready.get());
            assert_eq!(family, "Hack Nerd Font");
            Err("fixture failure\n\x1b[31m\u{202e}".into())
        },
    )
    .unwrap_err()
    .to_string();
    assert!(
        error.contains("Could not finish font selection") && error.contains("was not attempted"),
        "{error}"
    );
    assert!(error.contains("may remain"));
    assert!(!error.contains(['\n', '\x1b', '\u{202e}']));
    assert!(!env.managed_file("managed/ghostty/font.conf").exists());
    assert_eq!(
        fs::read(env.managed_file("current-font")).unwrap(),
        b"Old Mono"
    );
}

#[test]
fn font_flow_known_choices_skip_installation_and_cache_warnings_still_publish() {
    for choice in [
        ResolvedFontChoice::Installed("Private Mono".into()),
        ResolvedFontChoice::Catalog("Hack Nerd Font".into()),
    ] {
        let (_temp, env) = fixture();
        let installed = Cell::new(0);
        let cache = apply_with(
            &choice,
            &env,
            true,
            || {},
            |family| {
                assert_eq!(family, "Hack Nerd Font");
                installed.set(installed.get() + 1);
                Ok(FontCacheRefresh::Failed)
            },
        )
        .unwrap();
        if matches!(choice, ResolvedFontChoice::Installed(_)) {
            assert_eq!(installed.get(), 0);
            assert_eq!(cache, FontCacheRefresh::NotRequested);
        } else {
            assert_eq!(installed.get(), 1);
            assert_eq!(cache, FontCacheRefresh::Failed);
        }
        assert_eq!(
            fs::read_to_string(env.managed_file("current-font")).unwrap(),
            choice.font_name()
        );
        assert!(env.managed_file("managed/ghostty/font.conf").is_file());
        assert_eq!(
            crate::config::list_restore_points_with_env(&env)
                .unwrap()
                .len(),
            1
        );
    }
}

#[test]
fn font_flow_post_install_conflict_remains_an_error_with_recovery_guidance() {
    let (_temp, env) = fixture();
    let error = apply_with(
        &ResolvedFontChoice::Catalog("Hack Nerd Font".into()),
        &env,
        true,
        || {},
        |_| {
            fs::write(env.managed_file("current-font"), "External Mono").unwrap();
            Ok(FontCacheRefresh::Failed)
        },
    )
    .unwrap_err()
    .to_string();
    assert!(
        error.contains("slate restore") && error.contains("not automatically rolled back"),
        "{error}"
    );
    assert_eq!(
        fs::read(env.managed_file("current-font")).unwrap(),
        b"External Mono"
    );
    assert!(!env.managed_file("managed/ghostty/font.conf").exists());
}

#[test]
fn font_flow_success_feedback_respects_reminder_policy_and_escapes_display_only() {
    use crate::cli::new_shell_reminder::{
        reminder_flag_for_tests, reset_reminder_flag_for_tests, REMINDER_TEST_LOCK,
    };
    let _guard = REMINDER_TEST_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let (_temp, env) = fixture();
    for (auto, quiet) in [(true, false), (false, true), (true, true), (false, false)] {
        reset_reminder_flag_for_tests();
        emit_success(
            "Private Mono",
            &env,
            None,
            Options {
                auto,
                quiet,
                snapshot: true,
            },
            FontCacheRefresh::NotRequested,
        );
        assert_eq!(reminder_flag_for_tests(), !auto && !quiet);
    }
    reset_reminder_flag_for_tests();
    for format in [
        super::super::format_font_downloaded,
        super::super::format_font_updated,
    ] {
        let line = format(None, "Private\u{202e}Mono");
        assert!(!line.contains('\u{202e}'));
        assert!(line.contains("Private\\u{202e}Mono"));
    }
    assert_eq!(
        fs::read(env.managed_file("current-font")).unwrap(),
        b"Old Mono"
    );
}
