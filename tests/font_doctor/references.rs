use super::*;

fn references<'a>(report: &'a serde_json::Value, terminal: &str) -> &'a serde_json::Value {
    report["font_references"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["terminal"] == terminal)
        .unwrap()
}

#[test]
fn font_doctor_references_separate_generated_outputs_from_direct_entry_links() {
    let home = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(home.path().into());
    seed(&env);
    let empty = json(&mut command(&env, true));
    assert_eq!(checks(&empty, "output_matches").len(), 3);
    assert_eq!(checks(&empty, "font_ref_entry_missing").len(), 3);
    let ghostty = env.xdg_config_home().join("ghostty/config.ghostty");
    let alacritty = env.xdg_config_home().join("alacritty/alacritty.toml");
    let kitty = env.xdg_config_home().join("kitty/kitty.conf");
    for connected in [false, true] {
        let g = env.managed_file("managed/ghostty/font.conf");
        write(
            &ghostty,
            if connected {
                format!("\u{feff}config-file = ?{}\n", g.display())
            } else {
                format!("config-file = {}\nconfig-file =\n", g.display())
            },
        );
        let a = env.managed_file("managed/alacritty/font.toml");
        write(
            &alacritty,
            format!(
                "{}[general]\nimport = [\"{}\"]\n",
                if connected { "" } else { "import = []\n" },
                a.display()
            ),
        );
        let k = env.managed_file("managed/kitty/font.conf");
        write(
            &kitty,
            if connected {
                format!("include {}/\n\\font.conf\n", k.parent().unwrap().display())
            } else {
                format!("include /outside # {}\n", k.display())
            },
        );
        let before = snapshot::tree(home.path());
        let report = json(&mut command(&env, true));
        assert_eq!(checks(&report, "output_matches").len(), 3);
        assert_eq!(
            checks(
                &report,
                if connected {
                    "font_ref_found"
                } else {
                    "font_ref_not_found"
                }
            )
            .len(),
            3
        );
        for terminal in ["ghostty", "alacritty", "kitty"] {
            let summary = references(&report, terminal);
            assert_eq!(
                summary["state"],
                if connected { "found" } else { "not_found" }
            );
            assert_eq!(summary["inspection_complete"], true);
        }
        assert_eq!(snapshot::tree(home.path()), before);
    }
    let output = command(&env, true)
        .args(["doctor", "opacity", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let other: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert!(other.get("font_references").is_none());
}

#[test]
fn font_doctor_references_follow_readonly_links_and_preserve_partial_isolated_evidence() {
    let home = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(home.path().into());
    seed(&env);
    let ghostty = env.xdg_config_home().join("ghostty");
    fs::create_dir_all(&ghostty).unwrap();
    let g = format!(
        "config-file = {}\n",
        env.managed_file("managed/ghostty/font.conf").display()
    );
    write(&home.path().join("linked-ghostty"), &g);
    write(&outside.path().join("ghostty"), &g);
    symlink(home.path().join("linked-ghostty"), ghostty.join("config")).unwrap();
    symlink(
        outside.path().join("ghostty"),
        ghostty.join("config.ghostty"),
    )
    .unwrap();
    let alacritty = env.xdg_config_home().join("alacritty/alacritty.toml");
    fs::create_dir_all(alacritty.parent().unwrap()).unwrap();
    write(
        &home.path().join("linked-alacritty"),
        format!(
            "[general]\nimport = [\"{}\"]\n",
            env.managed_file("managed/alacritty/font.toml").display()
        ),
    );
    symlink(home.path().join("linked-alacritty"), &alacritty).unwrap();
    let kitty = env.xdg_config_home().join("kitty/kitty.conf");
    fs::create_dir_all(kitty.parent().unwrap()).unwrap();
    write(
        &outside.path().join("kitty"),
        format!(
            "include {}\n",
            env.managed_file("managed/kitty/font.conf").display()
        ),
    );
    symlink(outside.path().join("kitty"), &kitty).unwrap();
    let before = snapshot::tree(home.path());
    let outside_before = snapshot::tree(outside.path());
    let report = json(&mut command(&env, true));
    assert_eq!(references(&report, "ghostty")["state"], "found");
    assert_eq!(references(&report, "ghostty")["inspection_complete"], false);
    assert_eq!(references(&report, "alacritty")["state"], "found");
    assert_eq!(references(&report, "kitty")["state"], "uninspectable");
    assert_eq!(checks(&report, "font_ref_entry_unreadable").len(), 2);
    for entry in checks(&report, "font_ref_entry_unreadable") {
        assert!(entry["message"]
            .as_str()
            .unwrap()
            .contains("escapes isolated SLATE_HOME"));
    }
    // Outside links are legitimate read-only configuration in an ordinary
    // profile. This target still never starts native validation or installers.
    let ordinary = json(&mut command(&env, false));
    for terminal in ["ghostty", "alacritty", "kitty"] {
        assert_eq!(references(&ordinary, terminal)["state"], "found");
        assert_eq!(references(&ordinary, terminal)["inspection_complete"], true);
    }
    assert_eq!(snapshot::tree(home.path()), before);
    assert_eq!(snapshot::tree(outside.path()), outside_before);
}

#[test]
fn font_doctor_references_do_not_report_unsafe_or_unparseable_entries_as_unlinked() {
    for (relative, kind) in [
        ("ghostty/config.ghostty", "line_limit"),
        ("kitty/kitty.conf", "fifo"),
        ("kitty/kitty.conf", "binary"),
        ("alacritty/alacritty.toml", "toml"),
        ("kitty/kitty.conf", "large"),
    ] {
        let home = tempfile::tempdir().unwrap();
        let env = SlateEnv::with_home(home.path().into());
        seed(&env);
        let path = env.xdg_config_home().join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        match kind {
            "line_limit" => write(&path, "#".repeat(4095)),
            "fifo" => {
                let path = std::ffi::CString::new(path.as_os_str().as_encoded_bytes()).unwrap();
                assert_eq!(unsafe { libc::mkfifo(path.as_ptr(), 0o600) }, 0);
            }
            "binary" => write(&path, b"PRIVATE_CONTENT\xff"),
            "toml" => write(&path, "PRIVATE_CONTENT = ["),
            "large" => fs::File::create(&path)
                .unwrap()
                .set_len(8 * 1024 * 1024 + 1)
                .unwrap(),
            _ => unreachable!(),
        }
        let before = snapshot::tree(home.path());
        let report = json(&mut command(&env, true));
        assert_eq!(checks(&report, "output_matches").len(), 3, "{kind}");
        assert_eq!(
            checks(&report, "font_ref_entry_unreadable").len(),
            1,
            "{kind}"
        );
        assert!(checks(&report, "font_ref_not_found").is_empty(), "{kind}");
        assert_eq!(
            references(&report, relative.split('/').next().unwrap())["inspection_complete"],
            false
        );
        assert_eq!(snapshot::tree(home.path()), before);
    }
}
