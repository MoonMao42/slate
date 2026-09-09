use super::*;
use std::{fs, os::unix::fs::symlink};

fn empty() -> AutoConfig {
    AutoConfig {
        dark_theme: None,
        light_theme: None,
    }
}

#[test]
fn auto_resolution_covers_current_and_catalog_pairs_for_every_embedded_theme() {
    let registry = ThemeRegistry::new().unwrap();
    for theme in registry.all() {
        for appearance in [ThemeAppearance::Dark, ThemeAppearance::Light] {
            let choice = choose(&registry, &empty(), appearance, || {
                Ok(Some(theme.id.clone()))
            })
            .unwrap();
            let (id, source) = if theme.appearance == appearance {
                (theme.id.as_str(), ChoiceSource::CurrentTheme)
            } else if let Some(pair) = theme.auto_pair.as_deref() {
                (pair, ChoiceSource::CatalogPair)
            } else {
                (
                    if appearance == ThemeAppearance::Dark {
                        "catppuccin-mocha"
                    } else {
                        "catppuccin-latte"
                    },
                    ChoiceSource::BrandDefault,
                )
            };
            assert_eq!(choice.theme.id, id);
            assert_eq!(choice.source, source);
        }
    }
    let nord = choose(&registry, &empty(), ThemeAppearance::Light, || {
        Ok(Some("nord".into()))
    })
    .unwrap();
    assert_eq!(nord.theme.id, "nord");
    assert_eq!(nord.theme.appearance, ThemeAppearance::Dark);
    assert_eq!(nord.source, ChoiceSource::CatalogPair);
}

#[test]
fn auto_resolution_preserves_explicit_overrides_and_never_echoes_unknown_pairing_ids() {
    let registry = ThemeRegistry::new().unwrap();
    let pairing = AutoConfig {
        dark_theme: Some("PRIVATE_UNKNOWN\u{1b}[2J".into()),
        light_theme: Some("nord".into()),
    };
    let choice = choose(&registry, &pairing, ThemeAppearance::Light, || {
        panic!("explicit slot must not read current")
    })
    .unwrap();
    assert_eq!(choice.theme.id, "nord");
    assert_eq!(choice.theme.appearance, ThemeAppearance::Dark);
    assert_eq!(choice.source, ChoiceSource::Configured);
    let error = choose(&registry, &pairing, ThemeAppearance::Dark, || {
        panic!("bad explicit slot must not fall back")
    })
    .err()
    .unwrap()
    .to_string();
    assert!(error.contains("Saved dark auto-theme pairing"));
    assert!(!error.contains("PRIVATE_UNKNOWN"));
    assert!(!error.contains('\u{1b}'));
}

#[test]
fn auto_resolution_distinguishes_absent_and_unknown_current_defaults() {
    let registry = ThemeRegistry::new().unwrap();
    for appearance in [ThemeAppearance::Dark, ThemeAppearance::Light] {
        for current in [None, Some("PRIVATE_UNKNOWN".to_owned())] {
            let reason = if current.is_some() {
                FallbackReason::UnknownCurrentTheme
            } else {
                FallbackReason::NoCurrentTheme
            };
            let choice = choose(&registry, &empty(), appearance, || Ok(current)).unwrap();
            assert_eq!(choice.theme.appearance, appearance);
            assert_eq!(choice.source, ChoiceSource::BrandDefault);
            assert_eq!(choice.fallback_reason, Some(reason));
        }
    }
}

#[test]
fn auto_resolution_profile_reads_are_bounded_safe_and_lazy_for_explicit_slots() {
    let registry = ThemeRegistry::new().unwrap();
    for kind in ["invalid-utf8", "symlink", "directory", "oversize"] {
        let td = tempfile::tempdir().unwrap();
        let env = SlateEnv::with_home(td.path().into());
        fs::create_dir_all(env.config_dir()).unwrap();
        fs::write(env.managed_file("auto.toml"), b"dark_theme='nord'\n").unwrap();
        let current = env.managed_file("current");
        match kind {
            "invalid-utf8" => fs::write(&current, b"\xffPRIVATE_CONTENT").unwrap(),
            "symlink" => symlink(env.managed_file("auto.toml"), &current).unwrap(),
            "directory" => fs::create_dir(&current).unwrap(),
            "oversize" => fs::File::create(&current)
                .unwrap()
                .set_len(MAX_STATE_BYTES + 1)
                .unwrap(),
            _ => unreachable!(),
        }
        let choice = resolve(&env, &registry, ThemeAppearance::Dark).unwrap();
        assert_eq!(choice.theme.id, "nord");
        let error = resolve(&env, &registry, ThemeAppearance::Light)
            .err()
            .unwrap()
            .to_string();
        assert!(!error.contains("PRIVATE_CONTENT"));
        assert!(!env.slate_cache_dir().exists());
    }
    let td = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(td.path().into());
    fs::create_dir_all(env.config_dir()).unwrap();
    fs::write(env.managed_file("current"), " \n\t ").unwrap();
    assert!(read_current(&env).unwrap().is_none());
    symlink(env.managed_file("current"), env.managed_file("auto.toml")).unwrap();
    assert!(resolve(&env, &registry, ThemeAppearance::Dark).is_err());
}
