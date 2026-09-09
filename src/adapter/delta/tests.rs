use super::*;
use std::collections::HashMap;

fn create_test_palette() -> crate::theme::Palette {
    crate::theme::Palette {
        foreground: "#ffffff".to_string(),
        background: "#000000".to_string(),
        cursor: None,
        selection_bg: None,
        selection_fg: None,
        brand_accent: "#7287fd".to_string(),
        black: "#000000".to_string(),
        red: "#ff0000".to_string(),
        green: "#00ff00".to_string(),
        yellow: "#ffff00".to_string(),
        blue: "#0000ff".to_string(),
        magenta: "#ff00ff".to_string(),
        cyan: "#00ffff".to_string(),
        white: "#ffffff".to_string(),
        bright_black: "#808080".to_string(),
        bright_red: "#ff6b6b".to_string(),
        bright_green: "#69ff69".to_string(),
        bright_yellow: "#ffff69".to_string(),
        bright_blue: "#6b69ff".to_string(),
        bright_magenta: "#ff69ff".to_string(),
        bright_cyan: "#69ffff".to_string(),
        bright_white: "#ffffff".to_string(),
        bg_dim: None,
        bg_darker: None,
        bg_darkest: None,
        rosewater: None,
        flamingo: None,
        pink: None,
        mauve: None,
        lavender: None,
        text: None,
        subtext1: None,
        subtext0: None,
        overlay2: None,
        overlay1: None,
        overlay0: None,
        surface2: None,
        surface1: None,
        surface0: None,
        extras: HashMap::new(),
    }
}

fn create_test_theme() -> ThemeVariant {
    ThemeVariant {
        id: "test".to_string(),
        name: "Test Theme".to_string(),
        family: "Test".to_string(),
        palette: create_test_palette(),
        tool_refs: HashMap::from([
            ("ghostty".to_string(), "test".to_string()),
            ("alacritty".to_string(), "test".to_string()),
            ("bat".to_string(), "test".to_string()),
            ("delta".to_string(), "test".to_string()),
            ("starship".to_string(), "test".to_string()),
            ("eza".to_string(), "test".to_string()),
            ("lazygit".to_string(), "test".to_string()),
            ("fastfetch".to_string(), "test".to_string()),
            ("tmux".to_string(), "test".to_string()),
            ("zsh_syntax_highlighting".to_string(), "test".to_string()),
        ]),
        appearance: crate::theme::ThemeAppearance::Dark,
        auto_pair: None,
    }
}

#[test]
#[ignore = "requires explicit SLATE_BAT_BINARY and SLATE_DELTA_BINARY; isolated native cache build"]
fn delta_native_generated_bat_cache_resolves_every_syntax_theme() {
    use std::time::Duration;
    let bat = std::fs::canonicalize(std::env::var_os("SLATE_BAT_BINARY").expect("set Bat binary"))
        .unwrap();
    let delta =
        std::fs::canonicalize(std::env::var_os("SLATE_DELTA_BINARY").expect("set Delta binary"))
            .unwrap();
    let home = tempfile::tempdir().unwrap();
    let assets = home.path().join("bat");
    let cache = home.path().join("cache");
    std::fs::create_dir_all(assets.join("themes")).unwrap();
    let themes = crate::theme::ThemeRegistry::new().unwrap();
    for theme in themes.all() {
        std::fs::write(
            assets
                .join("themes")
                .join(format!("slate-{}.tmTheme", theme.id)),
            crate::adapter::bat::tmtheme::render_tmtheme(&theme.palette, &theme.id),
        )
        .unwrap();
    }
    let command = |binary: &Path| {
        let mut cmd = assert_cmd::Command::new(binary);
        cmd.env_clear()
            .env("HOME", home.path())
            .env("XDG_CONFIG_HOME", home.path())
            .env("XDG_CACHE_HOME", &cache)
            .env("BAT_CONFIG_DIR", &assets)
            // Delta 0.19.2 resolves XDG_CACHE_HOME/bat here rather than
            // following an arbitrary BAT_CACHE_PATH used by Bat.
            .env("BAT_CACHE_PATH", cache.join("bat"))
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", home.path().join(".gitconfig"))
            .current_dir(home.path())
            .timeout(Duration::from_secs(20));
        cmd
    };
    command(&bat).args(["cache", "--build"]).assert().success();
    let listed = command(&delta)
        .arg("--list-syntax-themes")
        .assert()
        .success()
        .stderr("")
        .get_output()
        .stdout
        .clone();
    let listed = String::from_utf8(listed).unwrap();
    let mut classification_mismatches = Vec::new();
    let diff = "diff --git a/fixture.rs b/fixture.rs\n--- a/fixture.rs\n+++ b/fixture.rs\n@@ -1 +1 @@\n-let value = 1;\n+let value = 2;\n";
    for theme in themes.all() {
        let name = &theme.tool_refs["delta"];
        let (appearance, flag, opposite) = match theme.appearance {
            crate::theme::ThemeAppearance::Light => ("light", "--light", "--dark"),
            crate::theme::ThemeAppearance::Dark => ("dark", "--dark", "--light"),
        };
        if !listed
            .lines()
            .any(|line| line.split_whitespace().collect::<Vec<_>>() == [appearance, name])
        {
            classification_mismatches.push(theme.id.clone());
        }
        assert!(
            listed
                .lines()
                .any(|line| line.split_whitespace().last() == Some(name)),
            "{} missing from native cache",
            theme.id
        );
        std::fs::write(
            home.path().join(".gitconfig"),
            DeltaAdapter::render_delta_colors(theme).unwrap(),
        )
        .unwrap();
        let output = command(&delta)
            .args(["--paging", "never", "--true-color", "always"])
            .write_stdin(diff)
            .assert()
            .success()
            .stderr("")
            .get_output()
            .stdout
            .clone();
        let output = String::from_utf8(output).unwrap();
        assert!(
            output.contains("value"),
            "empty or incomplete diff for {}",
            theme.id
        );
        assert!(
            output.contains('\x1b'),
            "expected colored diff for {}",
            theme.id
        );
        let reference = |mode: &str| {
            command(&delta)
                .args([
                    "--no-gitconfig",
                    "--paging",
                    "never",
                    "--true-color",
                    "always",
                    "--line-numbers",
                    "--syntax-theme",
                    name,
                    mode,
                ])
                .write_stdin(diff)
                .assert()
                .success()
                .stderr("")
                .get_output()
                .stdout
                .clone()
        };
        assert_eq!(
            output.as_bytes(),
            reference(flag),
            "saved appearance differs from explicit mode for {}",
            theme.id
        );
        assert_ne!(
            output.as_bytes(),
            reference(opposite),
            "fixture must distinguish light and dark for {}",
            theme.id
        );
    }
    eprintln!("Native list classification differs from Slate appearance for: {classification_mismatches:?}; rendered modes were independently verified.");
}

#[test]
fn delta_include_output_size_limit_accepts_boundary_and_rejects_growth_before_writes() {
    use crate::config::file_read::MAX_TOOL_CONFIG_BYTES;
    use std::os::unix::fs::MetadataExt;
    for extra in [0, 1] {
        let home = tempfile::tempdir().unwrap();
        let env = SlateEnv::with_home(home.path().to_owned());
        let path = home.path().join(".gitconfig");
        let managed = env.managed_file("managed/delta/colors");
        let theme = create_test_theme();
        let block = DeltaAdapter::render_delta_config(&theme, &managed).unwrap();
        let added = marker_block::upsert_managed_block_bytes(b"", block.as_bytes())
            .unwrap()
            .len();
        let size = MAX_TOOL_CONFIG_BYTES as usize - added + extra;
        let mut original = vec![b'x'; size];
        original[0] = b'#';
        original[size - 1] = b'\n';
        std::fs::write(&path, &original).unwrap();
        let inode = std::fs::metadata(&path).unwrap().ino();
        if extra == 1 {
            let error = DeltaAdapter
                .apply_theme_with_env(&theme, &env)
                .unwrap_err()
                .to_string();
            assert!(error.contains("would exceed the 8 MiB"));
            assert_eq!(std::fs::read(&path).unwrap(), original);
            assert_eq!(std::fs::metadata(&path).unwrap().ino(), inode);
            assert!(!env.config_dir().exists());
        } else {
            DeltaAdapter.apply_theme_with_env(&theme, &env).unwrap();
            assert_eq!(
                std::fs::metadata(&path).unwrap().len(),
                MAX_TOOL_CONFIG_BYTES
            );
            let first = std::fs::metadata(&path).unwrap().ino();
            DeltaAdapter.apply_theme_with_env(&theme, &env).unwrap();
            assert_eq!(std::fs::metadata(&path).unwrap().ino(), first);
            assert_eq!(
                marker_block::strip_managed_blocks_bytes(&std::fs::read(&path).unwrap()).unwrap(),
                original
            );
        }
    }
}

#[test]
fn delta_publication_refuses_changed_gitconfig_without_overwriting() {
    use crate::config::file_read::{self, Links, MAX_TOOL_CONFIG_BYTES};
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    for change in ["content", "mode", "replacement", "oversized", "missing"] {
        let home = tempfile::tempdir().unwrap();
        let env = SlateEnv::with_home(home.path().to_owned());
        let path = home.path().join(".gitconfig");
        std::fs::write(&path, "# original\n").unwrap();
        let destination =
            super::super::integration_publish::destination(&path, "Git configuration").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        let original = file_read::read(&path, MAX_TOOL_CONFIG_BYTES, Links::Reject)
            .unwrap()
            .unwrap();
        match change {
            "content" => std::fs::write(&path, "# PRIVATE_EXTERNAL\n").unwrap(),
            "mode" => {
                std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o640)).unwrap()
            }
            "replacement" => {
                let replacement = home.path().join("replacement");
                std::fs::write(&replacement, &original.bytes).unwrap();
                std::fs::rename(replacement, &path).unwrap();
            }
            "oversized" => std::fs::OpenOptions::new()
                .write(true)
                .open(&path)
                .unwrap()
                .set_len(MAX_TOOL_CONFIG_BYTES + 1)
                .unwrap(),
            _ => std::fs::remove_file(&path).unwrap(),
        }
        let before = std::fs::metadata(&path).ok();
        let error = super::super::integration_publish::publish(
            &env,
            &path,
            &destination,
            Some(&original),
            b"new include\n",
            "Git configuration",
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("was not overwritten"));
        assert!(!error.contains("PRIVATE_EXTERNAL"));
        if let Some(before) = before {
            let after = std::fs::metadata(&path).unwrap();
            assert_eq!(
                (before.ino(), before.len(), before.mode()),
                (after.ino(), after.len(), after.mode())
            );
            if change == "content" {
                assert_eq!(
                    std::fs::read_to_string(&path).unwrap(),
                    "# PRIVATE_EXTERNAL\n"
                );
            }
        } else {
            assert!(!path.exists());
        }
    }
}

#[test]
fn delta_unrepresentable_include_paths_fail_without_writes() {
    use std::os::unix::ffi::OsStringExt;
    for name in [
        b"PRIVATE\npath".to_vec(),
        b"PRIVATE\rpath".to_vec(),
        b"PRIVATE\tpath".to_vec(),
        b"PRIVATE\x1bpath".to_vec(),
        b"PRIVATE\xffpath".to_vec(),
    ] {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join(std::ffi::OsString::from_vec(name));
        if home.to_str().is_none() {
            // macOS can reject these bytes before a directory exists.
            // Exercise the encoder directly without weakening that case.
            let error = DeltaAdapter::format_gitconfig_include_path(&home)
                .unwrap_err()
                .to_string();
            assert!(error.contains("UTF-8 without control characters"));
            assert!(!error.contains("PRIVATE"));
            assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 0);
            continue;
        }
        std::fs::create_dir(&home).unwrap();
        let env = SlateEnv::with_home(home.clone());
        let path = home.join(".gitconfig");
        std::fs::write(&path, "# personal\n").unwrap();
        let error = DeltaAdapter
            .apply_theme_with_env(&create_test_theme(), &env)
            .unwrap_err()
            .to_string();
        assert!(error.contains("Delta include path must be UTF-8 without control characters"));
        assert!(!error.contains("PRIVATE"));
        assert_eq!(std::fs::read_to_string(path).unwrap(), "# personal\n");
        assert_eq!(std::fs::read_dir(home).unwrap().count(), 1);
    }
}

#[test]
fn delta_native_git_loads_managed_include_across_catalog_and_quoted_paths() {
    let temp = tempfile::tempdir().unwrap();
    // Exercise the path encoder through Git, not just a string assertion.
    let home = temp.path().join("Slate 中文 'fixture' \"quoted\" \\ path");
    std::fs::create_dir(&home).unwrap();
    let env = SlateEnv::with_home(home.clone());
    let path = home.join(".gitconfig");
    let personal = "[core]\n pager = custom-pager\n[user]\n name = Fixture User\n";
    std::fs::write(&path, personal).unwrap();
    let themes = crate::theme::ThemeRegistry::new().unwrap();
    for theme in themes.all() {
        DeltaAdapter.apply_theme_with_env(theme, &env).unwrap();
        let output = std::process::Command::new("git")
            .env_clear()
            .env("HOME", &home)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .current_dir(&home)
            .args(["config", "--includes", "--file"])
            .arg(&path)
            .arg("--list")
            .output()
            .unwrap();
        assert!(output.status.success(), "{}: {:?}", theme.id, output.stderr);
        assert!(output.stderr.is_empty());
        let text = String::from_utf8(output.stdout).unwrap();
        let lines: Vec<_> = text.lines().collect();
        assert!(lines.contains(&"core.pager=custom-pager"));
        assert!(lines.contains(&"user.name=Fixture User"));
        assert!(
            lines.contains(&format!("delta.syntax-theme={}", theme.tool_refs["delta"]).as_str())
        );
        assert_eq!(
            lines
                .iter()
                .filter(|line| line.starts_with("delta.syntax-theme="))
                .count(),
            1
        );
        let (active, absent) = match theme.appearance {
            crate::theme::ThemeAppearance::Light => ("delta.light=true", "delta.dark="),
            crate::theme::ThemeAppearance::Dark => ("delta.dark=true", "delta.light="),
        };
        assert!(lines.contains(&active));
        assert!(!lines.iter().any(|line| line.starts_with(absent)));
        assert!(lines.contains(&"delta.line-numbers=true"));
        assert_eq!(
            marker_block::strip_managed_blocks(&std::fs::read_to_string(&path).unwrap()),
            personal
        );
    }
}

#[test]
fn delta_unsafe_palette_target_does_not_add_git_include() {
    use std::os::unix::fs::symlink;
    for linked in [false, true] {
        let home = tempfile::tempdir().unwrap();
        let env = SlateEnv::with_home(home.path().to_owned());
        let gitconfig = home.path().join(".gitconfig");
        std::fs::write(&gitconfig, "# personal\n").unwrap();
        let managed = env.managed_file("managed/delta/colors");
        std::fs::create_dir_all(managed.parent().unwrap()).unwrap();
        if linked {
            symlink(&gitconfig, &managed).unwrap();
        } else {
            std::fs::File::create(&managed)
                .unwrap()
                .set_len(crate::config::file_read::MAX_TOOL_CONFIG_BYTES + 1)
                .unwrap();
        }
        assert!(DeltaAdapter
            .apply_theme_with_env(&create_test_theme(), &env)
            .is_err());
        assert_eq!(std::fs::read_to_string(&gitconfig).unwrap(), "# personal\n");
        assert_eq!(std::fs::read_dir(env.config_dir()).unwrap().count(), 1);
        if linked {
            assert_eq!(std::fs::read_link(&managed).unwrap(), gitconfig);
        } else {
            assert_eq!(
                std::fs::metadata(&managed).unwrap().len(),
                crate::config::file_read::MAX_TOOL_CONFIG_BYTES + 1
            );
        }
    }
}

#[test]
fn delta_sync_preserves_personal_git_settings_and_noop_file_identity() {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    let home = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(home.path().to_owned());
    let path = home.path().join(".gitconfig");
    let personal = "# personal\n[core]\n\tpager = custom-pager\n[user]\n\tname = Fixture User\n";
    std::fs::write(&path, personal).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o640)).unwrap();
    let themes = crate::theme::ThemeRegistry::new().unwrap();
    let managed = env.managed_file("managed/delta/colors");
    for id in ["nord", "catppuccin-latte"] {
        let theme = themes.get(id).unwrap();
        DeltaAdapter.apply_theme_with_env(theme, &env).unwrap();
        assert_eq!(
            std::fs::read_to_string(&managed).unwrap(),
            DeltaAdapter::render_delta_colors(theme).unwrap()
        );
        let integration = std::fs::read_to_string(&path).unwrap();
        assert_eq!(marker_block::strip_managed_blocks(&integration), personal);
        assert_eq!(std::fs::metadata(&path).unwrap().mode() & 0o777, 0o640);
        std::fs::set_permissions(&managed, std::fs::Permissions::from_mode(0o640)).unwrap();
        let before = [
            std::fs::metadata(&path).unwrap(),
            std::fs::metadata(&managed).unwrap(),
        ];
        DeltaAdapter.apply_theme_with_env(theme, &env).unwrap();
        for (file, metadata) in [&path, &managed].into_iter().zip(before) {
            let after = std::fs::metadata(file).unwrap();
            assert_eq!(metadata.ino(), after.ino());
            assert_eq!(metadata.modified().unwrap(), after.modified().unwrap());
            assert_eq!(after.mode() & 0o777, 0o640);
        }
    }
    let entries: Vec<_> = std::fs::read_dir(env.config_dir())
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    assert_eq!(entries, ["managed"]);
    assert!(!env.zshrc_path().exists());
}

#[test]
fn delta_unsafe_gitconfig_stops_before_initializing_managed_state() {
    use std::os::unix::fs::{symlink, MetadataExt};
    for case in ["markers", "oversized", "directory", "symlink", "dangling"] {
        let home = tempfile::tempdir().unwrap();
        let env = SlateEnv::with_home(home.path().to_owned());
        let path = home.path().join(".gitconfig");
        let target = home.path().join("personal");
        match case {
            "markers" => {
                std::fs::write(&path, format!("{}\nPRIVATE\n", marker_block::START)).unwrap()
            }
            "oversized" => std::fs::File::create(&path)
                .unwrap()
                .set_len(crate::config::file_read::MAX_TOOL_CONFIG_BYTES + 1)
                .unwrap(),
            "directory" => std::fs::create_dir(&path).unwrap(),
            _ => {
                if case == "symlink" {
                    std::fs::write(&target, "PRIVATE").unwrap();
                }
                symlink(&target, &path).unwrap();
            }
        }
        let before = std::fs::symlink_metadata(&path).unwrap();
        let error = DeltaAdapter
            .apply_theme_with_env(&create_test_theme(), &env)
            .unwrap_err()
            .to_string();
        assert!(!error.contains("PRIVATE"));
        let after = std::fs::symlink_metadata(&path).unwrap();
        assert_eq!(before.ino(), after.ino());
        assert_eq!(before.len(), after.len());
        assert!(!env.config_dir().exists());
        if case == "symlink" {
            assert_eq!(std::fs::read_to_string(target).unwrap(), "PRIVATE");
        }
    }
}

#[test]
fn delta_invalid_reference_does_not_write_or_select_a_fallback() {
    for reference in [
        None,
        Some(""),
        Some("  "),
        Some("PRIVATE\n[core]\npager = bad"),
        Some("bad\x1b"),
    ] {
        let home = tempfile::tempdir().unwrap();
        let env = SlateEnv::with_home(home.path().to_owned());
        let path = home.path().join(".gitconfig");
        std::fs::write(&path, "# personal\n").unwrap();
        let mut theme = create_test_theme();
        theme.tool_refs.remove("delta");
        if let Some(value) = reference {
            theme.tool_refs.insert("delta".into(), value.into());
        }
        let error = DeltaAdapter
            .apply_theme_with_env(&theme, &env)
            .unwrap_err()
            .to_string();
        assert!(error.contains("no fallback was selected"));
        assert!(!error.contains("PRIVATE"));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "# personal\n");
        assert_eq!(std::fs::read_dir(home.path()).unwrap().count(), 1);
    }
}

#[test]
fn delta_quoted_theme_names_roundtrip_through_git() {
    use std::io::Write;
    use std::process::{Command, Stdio};
    let themes = crate::theme::ThemeRegistry::new().unwrap();
    let mut variants: Vec<_> = themes.all().into_iter().cloned().collect();
    let mut custom = create_test_theme();
    custom
        .tool_refs
        .insert("delta".into(), " name #semi; \"quoted\" \\ theme ".into());
    variants.push(custom);
    let home = tempfile::tempdir().unwrap();
    for theme in variants {
        let config = DeltaAdapter::render_delta_colors(&theme).unwrap();
        let mut child = Command::new("git")
            .env_clear()
            .env("HOME", home.path())
            .current_dir(home.path())
            .args(["config", "--file", "-", "--get", "delta.syntax-theme"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(config.as_bytes())
            .unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(output.status.success(), "{:?}", output.stderr);
        assert_eq!(
            String::from_utf8(output.stdout).unwrap(),
            format!("{}\n", theme.tool_refs["delta"])
        );
    }
    assert_eq!(std::fs::read_dir(home.path()).unwrap().count(), 0);
}

#[test]
fn test_tool_name() {
    let adapter = DeltaAdapter;
    assert_eq!(adapter.tool_name(), "delta");
}

#[test]
fn test_apply_strategy() {
    let adapter = DeltaAdapter;
    assert_eq!(adapter.apply_strategy(), ApplyStrategy::WriteAndInclude);
}

#[test]
fn test_render_delta_config() {
    let theme = create_test_theme();
    let managed_path = PathBuf::from("/home/user/.config/slate/managed/delta");
    let output = DeltaAdapter::render_delta_config(&theme, &managed_path).unwrap();

    marker_block::validate_block_state(&output).unwrap();
    assert!(marker_block::strip_managed_blocks(&output).is_empty());
    assert!(output.contains(marker_block::START));
    assert!(output.contains(marker_block::END));
    assert!(output.contains("[include]"));
    assert!(output.contains(".config/slate/managed/delta"));
}

#[test]
fn test_render_delta_colors() {
    let theme = create_test_theme();
    let output = DeltaAdapter::render_delta_colors(&theme).unwrap();

    assert!(output.contains("[delta]"));
    assert!(output.contains("syntax-theme = \"test\""));
    assert!(output.contains("dark = true"));
    assert!(
        !output.contains("light = true"),
        "Dark theme must not emit light = true"
    );
}

#[test]
fn test_render_delta_colors_emits_light_for_light_themes() {
    let mut theme = create_test_theme();
    theme.appearance = crate::theme::ThemeAppearance::Light;
    let output = DeltaAdapter::render_delta_colors(&theme).unwrap();

    assert!(
        output.contains("light = true"),
        "Light theme must emit light = true so delta picks light-bg defaults; got:\n{output}"
    );
    assert!(
        !output.contains("dark = true"),
        "Light theme must not emit dark = true (was the bug — washed out context lines on cream Ghostty bg); got:\n{output}"
    );
}

#[test]
fn apply_theme_skips_without_creating_missing_gitconfig() {
    let tempdir = tempfile::TempDir::new().unwrap();
    let env = SlateEnv::with_home(tempdir.path().to_path_buf());
    let adapter = DeltaAdapter;
    let theme = create_test_theme();

    let outcome = adapter.apply_theme_with_env(&theme, &env).unwrap();

    assert_eq!(
        outcome,
        ApplyOutcome::Skipped(crate::adapter::SkipReason::MissingIntegrationConfig)
    );
    assert!(!env.home().join(".gitconfig").exists());
    assert!(!env.config_dir().join("managed/delta/colors").exists());
}

#[test]
fn test_is_installed() {
    let adapter = DeltaAdapter;
    let _result = adapter.is_installed();
}
