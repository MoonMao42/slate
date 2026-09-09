use super::*;

#[test]
fn branch_layout_keeps_branch_without_status_or_duration_modules() {
    let theme = crate::theme::ThemeRegistry::new().unwrap();
    let output = render(
        "[git_status]\ndisabled = false\n",
        theme.get("nord").unwrap(),
        PromptStyle::Branch,
    )
    .unwrap();
    let doc: DocumentMut = output.parse().unwrap();
    assert_eq!(
        doc["format"].as_str(),
        Some("$directory$git_branch$character")
    );
    assert_eq!(doc["git_status"]["disabled"].as_bool(), Some(false));
    assert_eq!(doc["add_newline"].as_bool(), Some(false));
    assert_eq!(doc["right_format"].as_str(), Some(""));
    assert!(!matches_layout(&output, PromptStyle::Compact, false).unwrap());
}

#[test]
#[ignore = "explicit installed Starship; only a disposable repository/config"]
fn branch_native_prompt_shows_branch_on_one_line() {
    let binary = std::env::var_os("SLATE_STARSHIP_BINARY").expect("provide Starship path");
    let home = tempfile::tempdir().unwrap();
    let repo = home.path().join("branch-project");
    std::fs::create_dir(&repo).unwrap();
    assert!(std::process::Command::new("/usr/bin/git")
        .env_clear()
        .env("HOME", home.path())
        .args(["init", "-b", "slate-demo"])
        .current_dir(&repo)
        .output()
        .unwrap()
        .status
        .success());
    let themes = crate::theme::ThemeRegistry::new().unwrap();
    let config = home.path().join("starship.toml");
    std::fs::write(
        &config,
        render("", themes.get("nord").unwrap(), PromptStyle::Branch).unwrap(),
    )
    .unwrap();
    let output = assert_cmd::Command::new(binary)
        .env_clear()
        .env("HOME", home.path())
        .env("PATH", "/usr/bin:/bin")
        .env("STARSHIP_CONFIG", &config)
        .env("STARSHIP_CACHE", home.path().join("cache"))
        .env("STARSHIP_SHELL", "bash")
        .env("TERM", "xterm-256color")
        .current_dir(&repo)
        .args(["prompt", "--status", "0", "--terminal-width", "120"])
        .timeout(std::time::Duration::from_secs(5))
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8(output).unwrap();
    let text = console::strip_ansi_codes(&text);
    assert!(
        text.contains("branch-project") && text.contains("slate-demo"),
        "{text}"
    );
    assert!(!text.contains('\n'), "{text}");
}

#[test]
#[ignore = "requires an explicit native Starship binary; isolated configuration only"]
fn focus_native_prompt_is_single_line_across_themes_and_command_statuses() {
    let binary = std::env::var_os("SLATE_STARSHIP_BINARY").expect("provide Starship path");
    let home = tempfile::tempdir().unwrap();
    let directory = home.path().join("focus-project");
    std::fs::create_dir(&directory).unwrap();
    let config = home.path().join("starship.toml");
    for theme in crate::theme::ThemeRegistry::new().unwrap().all() {
        let contents = render("", theme, PromptStyle::Focus).unwrap();
        let doc: DocumentMut = contents.parse().unwrap();
        assert_eq!(doc["format"].as_str(), Some("$directory$character"));
        assert_eq!(doc["right_format"].as_str(), Some(""));
        std::fs::write(&config, contents).unwrap();
        for status in ["0", "1"] {
            let output = assert_cmd::Command::new(&binary)
                .env_clear()
                .env("HOME", home.path())
                .env("PATH", "")
                .env("STARSHIP_CONFIG", &config)
                .env("STARSHIP_CACHE", home.path().join("cache"))
                .env("STARSHIP_SHELL", "bash")
                .env("TERM", "xterm-256color")
                .current_dir(&directory)
                .args(["prompt", "--status", status, "--terminal-width", "120"])
                .timeout(std::time::Duration::from_secs(5))
                .assert()
                .success()
                .get_output()
                .clone();
            assert!(
                output.stderr.is_empty(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            let text = String::from_utf8(output.stdout).unwrap();
            assert!(text.contains("focus-project"));
            assert!(!text.contains('\n'));
            assert!(text.contains(if status == "0" { ">" } else { "x" }));
            let color = if status == "0" {
                &theme.palette.green
            } else {
                &theme.palette.red
            };
            let rgb = u32::from_str_radix(color.trim_start_matches('#'), 16).unwrap();
            assert!(
                text.contains(&format!(
                    "38;2;{};{};{}",
                    (rgb >> 16) & 255,
                    (rgb >> 8) & 255,
                    rgb & 255
                )),
                "{} status {status}: {text:?}",
                theme.id
            );
        }
    }
}

#[test]
fn prompt_styles_have_distinct_layouts_and_valid_palettes_for_every_theme() {
    for theme in crate::theme::ThemeRegistry::new().unwrap().all() {
        for style in PromptStyle::ALL {
            let output = render("", theme, style).unwrap();
            assert!(matches_layout(&output, style, false).unwrap());
            let plain = if style == PromptStyle::Rainbow {
                super::super::super::shell_integration::themed_plain_starship_content(theme)
            } else {
                output.clone()
            };
            assert!(matches_layout(&plain, style, true).unwrap());
            let doc: DocumentMut = output.parse().unwrap();
            assert_eq!(
                doc["palettes"]["slate"]["blue"].as_str(),
                Some(theme.palette.blue.as_str())
            );
            let format = doc["format"].as_str().unwrap();
            assert_eq!(
                format.contains("$line_break"),
                !matches!(
                    style,
                    PromptStyle::Compact | PromptStyle::Focus | PromptStyle::Branch
                )
            );
            assert_eq!(format.contains("$os"), style == PromptStyle::Rainbow);
            assert_eq!(
                doc["add_newline"].as_bool(),
                Some(!matches!(
                    style,
                    PromptStyle::Compact | PromptStyle::Focus | PromptStyle::Branch
                ))
            );
            if style != PromptStyle::Rainbow {
                assert!(output.is_ascii());
            }
        }
    }
}

#[test]
fn prompt_style_overlay_preserves_nonpresentation_settings_and_custom_definitions() {
    let input = "# personal\ncommand_timeout = 740\nright_format = '$custom'\n[directory]\ntruncation_length = 7\n[directory.substitutions]\n'Work' = 'Mine'\n[custom.keep]\ncommand = 'echo fg:crust'\nwhen = false\n[aws]\ndisabled = false\n[palettes.slate]\nprivate_accent = '#123456'\n";
    let theme = crate::theme::ThemeRegistry::new().unwrap();
    for style in PromptStyle::ALL {
        let result = render(input, theme.get("nord").unwrap(), style).unwrap();
        let doc: DocumentMut = result.parse().unwrap();
        assert!(result.contains("# personal"));
        assert_eq!(doc["command_timeout"].as_integer(), Some(740));
        assert_eq!(doc["directory"]["truncation_length"].as_integer(), Some(7));
        assert_eq!(
            doc["directory"]["substitutions"]["Work"].as_str(),
            Some("Mine")
        );
        assert_eq!(
            doc["custom"]["keep"]["command"].as_str(),
            Some("echo fg:crust")
        );
        assert_eq!(doc["aws"]["disabled"].as_bool(), Some(false));
        assert_eq!(
            doc["palettes"]["slate"]["private_accent"].as_str(),
            Some("#123456")
        );
        assert_eq!(
            render(&result, theme.get("nord").unwrap(), style).unwrap(),
            result
        );
    }
}

#[test]
fn classic_preserves_host_rules_and_aliases_and_uses_a_literal_dollar_prompt() {
    let input = r#"
username = { show_always = false, detect_env_vars = ['PERSONAL_USER'], aliases = { root = 'admin' } }
hostname = { ssh_only = false, disabled = true, trim_at = '', detect_env_vars = ['PERSONAL_HOST'], aliases = { workstation = 'desk' }, format = 'old' }
"#;
    let themes = crate::theme::ThemeRegistry::new().unwrap();
    let output = render(input, themes.get("nord").unwrap(), PromptStyle::Classic).unwrap();
    assert!(matches_layout(&output, PromptStyle::Classic, false).unwrap());
    let doc: DocumentMut = output.parse().unwrap();
    assert_eq!(doc["username"]["show_always"].as_bool(), Some(true));
    assert_eq!(doc["username"]["aliases"]["root"].as_str(), Some("admin"));
    assert_eq!(
        doc["username"]["detect_env_vars"][0].as_str(),
        Some("PERSONAL_USER")
    );
    assert_eq!(doc["hostname"]["ssh_only"].as_bool(), Some(false));
    assert_eq!(doc["hostname"]["disabled"].as_bool(), Some(true));
    assert_eq!(doc["hostname"]["trim_at"].as_str(), Some(""));
    assert_eq!(
        doc["hostname"]["aliases"]["workstation"].as_str(),
        Some("desk")
    );
    assert_eq!(
        doc["hostname"]["detect_env_vars"][0].as_str(),
        Some("PERSONAL_HOST")
    );
    assert_eq!(
        doc["hostname"]["format"].as_str(),
        Some("[@$hostname]($style)")
    );
    assert_eq!(
        doc["character"]["success_symbol"].as_str(),
        Some(r"[\$](bold green)")
    );
    assert_eq!(
        doc["character"]["error_symbol"].as_str(),
        Some(r"[\$](bold red)")
    );
    assert_eq!(doc["character"]["format"].as_str(), Some("$symbol "));
}

#[test]
fn prompt_styles_support_inline_tables_and_reject_wrong_table_shapes() {
    let themes = crate::theme::ThemeRegistry::new().unwrap();
    let theme = themes.get("nord").unwrap();
    for style in PromptStyle::ALL {
        let result = render(
            "directory = { truncation_length = 9 }\npalettes = { slate = { extra = '#123456' } }\n",
            theme,
            style,
        )
        .unwrap();
        let doc: DocumentMut = result.parse().unwrap();
        assert_eq!(doc["directory"]["truncation_length"].as_integer(), Some(9));
        assert_eq!(doc["palettes"]["slate"]["extra"].as_str(), Some("#123456"));
        for invalid in ["directory = 8", "palettes = 8", "[palettes]\nslate = 8"] {
            assert!(render(invalid, theme, style).is_err());
        }
    }
}
