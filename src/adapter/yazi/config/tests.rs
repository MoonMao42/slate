use super::*;
use std::{
    fs,
    os::unix::fs::{symlink, MetadataExt, PermissionsExt},
};

fn write(path: &std::path::Path, bytes: impl AsRef<[u8]>) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
}

#[test]
fn yazi_palette_covers_all_themes_and_keeps_syntax_assets_owned() {
    for theme in crate::theme::ThemeRegistry::new().unwrap().all() {
        let (styles, xml) = palette::render(theme).unwrap();
        assert!(owns_flavor(styles.as_bytes()) && owns_syntax(xml.as_bytes()));
        let doc: toml::Value = styles.parse().unwrap();
        assert_eq!(
            doc["app"]["overall"]["bg"].as_str(),
            Some(theme.palette.background.as_str())
        );
        assert_eq!(
            doc["mgr"]["cwd"]["fg"].as_str(),
            Some(theme.palette.cyan.as_str())
        );
        assert_eq!(doc["filetype"]["rules"].as_array().unwrap().len(), 4);
        for name in [
            "tabs",
            "mode",
            "indicator",
            "status",
            "confirm",
            "spot",
            "notify",
            "pick",
            "input",
            "cmp",
            "tasks",
            "help",
        ] {
            assert!(doc[name].is_table(), "{name}");
        }
        assert!(xml.contains(&theme.palette.background) && xml.contains(&theme.palette.foreground));
        assert_eq!(palette::render(theme).unwrap(), (styles, xml));
    }
}

#[test]
fn yazi_selection_preserves_personal_fields_comments_and_inline_tables() {
    for original in [
        "# personal\n[flavor]\ndark = 'personal' # keep this\nlight='personal-light'\n[mgr]\ncwd={fg='#123456',bold=true}\n[icon]\nprepend_dirs=[]\n",
        "flavor={ dark='personal', light='personal-light', future=3 } # keep this\n[custom]\ncommand='PRIVATE_CONTENT'\n",
    ] {
        let output = set_flavor(original.as_bytes()).unwrap();
        let output = std::str::from_utf8(&output).unwrap();
        assert!(output.contains("# keep this"));
        let mut before: toml::Value = original.parse().unwrap();
        before["flavor"]["dark"] = FLAVOR.into();
        before["flavor"]["light"] = FLAVOR.into();
        assert_eq!(output.parse::<toml::Value>().unwrap(), before);
        assert_eq!(set_flavor(output.as_bytes()).unwrap(), output.as_bytes());
        let cleaned = clean_config(output.as_bytes()).unwrap();
        before["flavor"].as_table_mut().unwrap().remove("dark");
        before["flavor"].as_table_mut().unwrap().remove("light");
        assert_eq!(std::str::from_utf8(&cleaned).unwrap().parse::<toml::Value>().unwrap(), before);
    }
    let unrelated = b"[flavor]\ndark='personal'\nlight='slate-sync'\n";
    let cleaned = clean_config(unrelated).unwrap();
    assert!(std::str::from_utf8(&cleaned)
        .unwrap()
        .contains("dark='personal'"));
}

#[test]
fn yazi_apply_is_idempotent_and_checks_all_inputs_before_asset_writes() {
    let theme = crate::theme::ThemeRegistry::new()
        .unwrap()
        .get("nord")
        .unwrap()
        .clone();
    let home = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(home.path().to_owned());
    let path = YaziAdapter::config_path(&env);
    write(&path, "# personal\n[flavor]\ndark='mine'\n");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();
    apply(&env, &theme).unwrap();
    let original =
        YaziAdapter::paths(&env).map(|p| (fs::metadata(&p).unwrap().ino(), fs::read(p).unwrap()));
    apply(&env, &theme).unwrap();
    assert_eq!(
        YaziAdapter::paths(&env).map(|p| (fs::metadata(&p).unwrap().ino(), fs::read(p).unwrap())),
        original
    );
    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o640
    );
    for bytes in [
        b"[PRIVATE_CONTENT".as_slice(),
        b"flavor=1",
        b"[flavor]\ndark=1",
        b"PRIVATE_CONTENT\xff",
    ] {
        write(&path, bytes);
        let assets = [
            YaziAdapter::flavor_path(&env),
            YaziAdapter::syntax_path(&env),
        ]
        .map(|p| fs::read(p).unwrap());
        assert!(apply(&env, &theme).is_err());
        assert_eq!(fs::read(&path).unwrap(), bytes);
        assert_eq!(
            [
                YaziAdapter::flavor_path(&env),
                YaziAdapter::syntax_path(&env)
            ]
            .map(|p| fs::read(p).unwrap()),
            assets
        );
    }
}

#[test]
fn yazi_foreign_assets_and_redirected_destinations_are_preserved() {
    for kind in ["foreign", "link", "outside", "changed", "fifo", "oversize"] {
        let home = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let env = SlateEnv::with_home(home.path().to_owned());
        let path = YaziAdapter::syntax_path(&env);
        write(&path, "PRIVATE_CONTENT");
        match kind {
            "fifo" => {
                fs::remove_file(&path).unwrap();
                let name = std::ffi::CString::new(path.as_os_str().as_encoded_bytes()).unwrap();
                assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
            }
            "oversize" => fs::File::create(&path)
                .unwrap()
                .set_len(MAX_TOOL_CONFIG_BYTES + 1)
                .unwrap(),
            "link" => {
                fs::remove_file(&path).unwrap();
                symlink(outside.path().join("missing"), &path).unwrap();
            }
            "outside" => {
                fs::rename(env.yazi_config_home(), home.path().join("original-yazi")).unwrap();
                symlink(outside.path(), env.yazi_config_home()).unwrap();
            }
            "changed" => {
                let file = File::read(&env, path.clone()).unwrap();
                write(&path, "EXTERNAL_EDIT");
                assert!(file.verify(&env).is_err());
            }
            _ => {}
        }
        assert!(apply(
            &env,
            crate::theme::ThemeRegistry::new()
                .unwrap()
                .get("nord")
                .unwrap()
        )
        .is_err());
        assert!(!YaziAdapter::config_path(&env).exists());
        assert!(!YaziAdapter::flavor_path(&env).exists());
        assert_eq!(fs::read_dir(outside.path()).unwrap().count(), 0);
    }
}
