use super::*;

fn managed(home: &Path) -> std::path::PathBuf {
    home.join(".config/slate/managed/ghostty/theme.conf")
}

#[test]
fn ghostty_doctor_window_style_reports_legacy_output_and_clears_after_regeneration() {
    use slate_cli::adapter::{GhosttyAdapter, ToolAdapter};
    use slate_cli::theme::ThemeRegistry;

    let td = tempfile::tempdir().unwrap();
    let root = entry(td.path());
    let theme = managed(td.path());
    write(&root, "config-file = user-window.conf\n");
    let user = root.parent().unwrap().join("user-window.conf");
    let original = format!(
        "macos-titlebar-style = tabs\nconfig-file = {}\n",
        theme.display()
    );
    write(&user, &original);
    write(&theme, "\u{feff}# legacy theme\r\n\tmacos-titlebar-style = \"transparent\"\r\nbackground = #1e1e2e\n");
    fs::set_permissions(&theme, fs::Permissions::from_mode(0o640)).unwrap();
    let before = tree_snapshot::tree(td.path());
    let json = report(td.path());
    let layout = &json["window_style"];
    assert_eq!(layout["status"], "managed_override");
    assert_eq!(layout["inspection_complete"], true);
    assert_eq!(layout["applies_to"], "macos");
    assert_eq!(layout["managed_overrides"].as_array().unwrap().len(), 1);
    let found = &layout["managed_overrides"][0];
    assert_eq!(found["first_assignment_line"], 2);
    assert_eq!(
        fs::canonicalize(found["path"].as_str().unwrap()).unwrap(),
        fs::canonicalize(&theme).unwrap()
    );
    assert_eq!(found["path_is_lossy"], false);
    assert_eq!(json["scan_complete"], true);
    assert_eq!(json["scan_issues"], serde_json::json!([]));
    assert_eq!(json["cycle_risk"], false);
    let text = command(td.path())
        .args(["doctor", "ghostty"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8(text).unwrap();
    assert!(
        text.contains("window layout: managed override found"),
        "{text}"
    );
    assert!(text.contains("first assignment at line 2"), "{text}");
    assert!(text.contains("Reapply the desired theme"), "{text}");
    assert!(
        text.contains("not proof of the effective style or live appearance"),
        "{text}"
    );
    assert!(
        !text.lines().any(|line| line == "scan incomplete:"),
        "{text}"
    );
    assert_eq!(tree_snapshot::tree(td.path()), before);

    // Exercise the real adapter's upgrade path inside this private profile.
    let env = SlateEnv::with_home(td.path().to_owned());
    let registry = ThemeRegistry::new().unwrap();
    GhosttyAdapter
        .apply_theme_with_env(registry.get("catppuccin-mocha").unwrap(), &env)
        .unwrap();
    assert_eq!(fs::read_to_string(&user).unwrap(), original);
    assert!(!fs::read_to_string(&theme)
        .unwrap()
        .contains("macos-titlebar-style"));
    let before = tree_snapshot::tree(td.path());
    let json = report(td.path());
    assert_eq!(json["window_style"]["status"], "not_found");
    assert_eq!(
        json["window_style"]["managed_overrides"],
        serde_json::json!([])
    );
    assert_eq!(tree_snapshot::tree(td.path()), before);
}

#[test]
fn ghostty_doctor_window_style_does_not_mistake_user_preferences_or_unloaded_files() {
    let td = tempfile::tempdir().unwrap();
    let root = entry(td.path());
    let theme = managed(td.path());
    write(&theme, "macos-titlebar-style = transparent\n");
    let user = root.parent().unwrap().join("user-window.conf");
    write(&user, "macos-titlebar-style = tabs\n");
    let lookalike = td
        .path()
        .join(".config/slate/managed-old/ghostty/theme.conf");
    write(&lookalike, "macos-titlebar-style = hidden\n");
    for content in [
        "macos-titlebar-style = native\nconfig-file = user-window.conf\n".to_owned(),
        format!("config-file = {}\n", lookalike.display()),
        format!("include = {}\n", theme.display()),
        format!("config-file = {}\nconfig-file =\n", theme.display()),
    ] {
        write(&root, content);
        let before = tree_snapshot::tree(td.path());
        let json = report(td.path());
        assert_eq!(
            json["window_style"]["managed_overrides"],
            serde_json::json!([]),
            "{json}"
        );
        assert_eq!(
            json["window_style"]["inspection_complete"],
            json["scan_complete"]
        );
        let expected = if json["scan_complete"] == true {
            "not_found"
        } else {
            "unknown"
        };
        assert_eq!(json["window_style"]["status"], expected);
        assert_eq!(tree_snapshot::tree(td.path()), before);
    }
    write(&root, format!("config-file = {}\n", theme.display()));
    for content in [
        "# macos-titlebar-style = transparent\n",
        "macos-titlebar-style-extra = transparent\n",
        "title = macos-titlebar-style = transparent\n",
        "macos-titlebar-style\n",
    ] {
        write(&theme, content);
        assert_eq!(report(td.path())["window_style"]["status"], "not_found");
    }

    // A link from the managed location must not reclassify a user's own file.
    let linked = tempfile::tempdir().unwrap();
    let user = linked.path().join("my-window.conf");
    let theme = managed(linked.path());
    write(&user, "macos-titlebar-style = tabs\n");
    fs::create_dir_all(theme.parent().unwrap()).unwrap();
    symlink(&user, &theme).unwrap();
    write(
        &entry(linked.path()),
        format!(
            "config-file = {}\nconfig-file = {}\n",
            user.display(),
            theme.display()
        ),
    );
    let before = tree_snapshot::tree(linked.path());
    assert_eq!(report(linked.path())["window_style"]["status"], "not_found");
    assert_eq!(tree_snapshot::tree(linked.path()), before);
}

#[test]
fn ghostty_doctor_window_style_keeps_bounded_positive_evidence_without_leaking_values() {
    let td = tempfile::tempdir().unwrap();
    let root = entry(td.path());
    let theme = managed(td.path());
    let alias = root.parent().unwrap().join("linked-theme.conf");
    write(
        &theme,
        "macos-titlebar-style = PRIVATE_CONTENT\u{1b}[31m\nmacos-titlebar-style = tabs\n",
    );
    fs::create_dir_all(root.parent().unwrap()).unwrap();
    symlink(&theme, &alias).unwrap();
    write(
        &root,
        format!(
            "config-file = {}\nconfig-file = {}\nconfig-file = absent\n",
            theme.display(),
            alias.display()
        ),
    );
    let before = tree_snapshot::tree(td.path());
    let json = report(td.path());
    let layout = &json["window_style"];
    assert_eq!(layout["status"], "managed_override");
    assert_eq!(layout["inspection_complete"], false);
    assert_eq!(layout["managed_overrides"].as_array().unwrap().len(), 1);
    assert_eq!(layout["managed_overrides"][0]["first_assignment_line"], 1);
    let text = command(td.path())
        .args(["doctor", "ghostty"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8(text).unwrap();
    assert!(!text.contains("PRIVATE_CONTENT"));
    assert!(!text.contains('\u{1b}'));
    assert!(text.contains("scan incomplete"));
    assert_eq!(tree_snapshot::tree(td.path()), before);

    // Do not inspect assignments beyond the native line-reader limit or bad UTF-8.
    for bytes in [
        format!(
            "#{}\nmacos-titlebar-style = transparent\n",
            "x".repeat(4094)
        )
        .into_bytes(),
        b"\xff\nmacos-titlebar-style = transparent\n".to_vec(),
    ] {
        write(&theme, bytes);
        let before = tree_snapshot::tree(td.path());
        let json = report(td.path());
        assert_eq!(json["window_style"]["status"], "unknown");
        assert_eq!(
            json["window_style"]["managed_overrides"],
            serde_json::json!([])
        );
        assert_eq!(tree_snapshot::tree(td.path()), before);
    }
}
