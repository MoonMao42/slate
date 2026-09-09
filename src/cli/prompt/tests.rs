use super::*;
use clap::Parser;

#[derive(Parser)]
struct PromptCli {
    #[command(flatten)]
    options: PromptOptions,
}

#[test]
fn catalog_language_is_explicit_text_only_and_never_mutation_consent() {
    for language in ["zh-CN", "en"] {
        let args = PromptCli::try_parse_from(["prompt", "--list", "--language", language]).unwrap();
        assert_eq!(args.options.language.as_deref(), Some(language));
        assert!(args.options.list && !args.options.yes && !args.options.json);
        assert!(PromptCli::try_parse_from(["prompt", "--language", language]).is_err());
        assert!(
            PromptCli::try_parse_from(["prompt", "focus", "--yes", "--language", language])
                .is_err()
        );
        assert!(
            PromptCli::try_parse_from(["prompt", "--list", "--json", "--language", language])
                .is_err()
        );
    }
    assert!(PromptCli::try_parse_from(["prompt", "--list", "--language", "unknown"]).is_err());
    use crate::config::ui_language::UiLanguage;
    let en = catalog_text(UiLanguage::English);
    let zh = catalog_text(UiLanguage::Chinese);
    for style in PromptStyle::ALL {
        assert!(en.contains(&format!(
            "{} — {}\n{}\n{}\n",
            style.id(),
            style.label(),
            style.description(),
            style.sample()
        )));
        assert!(zh.contains(&format!(
            "{} — {}",
            style.id(),
            style_label_in(style, UiLanguage::Chinese)
        )));
        assert!(zh.contains(style.sample()));
    }
    assert!(zh.starts_with("提示符样式 · 仅为示意"));
}

#[test]
fn prompt_review_reports_captured_override_without_reading_or_targeting_it() {
    use std::os::unix::ffi::OsStringExt;
    let root = tempfile::tempdir().unwrap();
    for value in [
        std::ffi::OsString::from("relative/personal\n\x1b[2J.toml"),
        std::ffi::OsString::from_vec(b"/unread/invalid-\xff.toml".to_vec()),
    ] {
        let env = SlateEnv::from_vars(|key| match key {
            "HOME" => Some(root.path().as_os_str().to_owned()),
            "STARSHIP_CONFIG" => Some(value.clone()),
            _ => None,
        })
        .unwrap();
        std::fs::create_dir_all(env.config_dir()).unwrap();
        std::fs::write(env.managed_file("current"), "nord\n").unwrap();
        let plan = PreparedPrompt::capture(&env, PromptStyle::Focus).unwrap();
        let preview = plan.preview();
        let selected = preview.starship_config_override.as_ref().unwrap();
        assert_eq!(selected.path, value.to_string_lossy());
        assert_eq!(selected.path_is_lossy, value.to_str().is_none());
        assert_eq!(preview.changes.len(), 3);
        assert!(!preview
            .changes
            .iter()
            .any(|change| change.path.as_os_str() == value));
        let text = preview.render();
        assert!(text.contains("Captured STARSHIP_CONFIG:"));
        assert!(text.contains("Only the file targets listed below are updated"));
        assert!(!text.contains('\x1b'));
        assert!(!text.contains("personal\n"));
        let compact = preview.render_review(false);
        assert!(compact.len() < text.len());
        assert!(compact.contains("部分写入失败不会自动回滚"));
        assert!(compact.contains("完整明细：slate prompt focus --dry-run"));
        assert!(!compact.contains('\x1b'));
        for change in preview.changes.iter().filter(|change| change.changed) {
            assert!(compact.contains(&change.path.to_string_lossy().to_string()));
        }
        let json = serde_json::to_value(&preview).unwrap();
        assert_eq!(
            json["starship_config_override"]["path_is_lossy"],
            value.to_str().is_none()
        );
        assert_eq!(std::fs::read_dir(env.config_dir()).unwrap().count(), 1);
    }
    let isolated = SlateEnv::from_vars(|key| match key {
        "SLATE_HOME" => Some(root.path().as_os_str().to_owned()),
        "STARSHIP_CONFIG" => Some("/host/ignored.toml".into()),
        _ => None,
    })
    .unwrap();
    assert!(PreparedPrompt::capture(&isolated, PromptStyle::Focus)
        .unwrap()
        .preview()
        .starship_config_override
        .is_none());
}

#[test]
fn compact_review_lists_only_writes_and_does_not_repeat_the_example() {
    let root = tempfile::tempdir().unwrap();
    let env = SlateEnv::from_vars(|key| {
        (key == "SLATE_HOME").then(|| root.path().as_os_str().to_owned())
    })
    .unwrap();
    std::fs::create_dir_all(env.config_dir()).unwrap();
    std::fs::write(env.managed_file("current"), "nord\n").unwrap();
    let mut preview = PreparedPrompt::capture(&env, PromptStyle::Focus)
        .unwrap()
        .preview();
    // Mixed plans should name writes without burying them among unchanged files.
    preview.changes[0].changed = true;
    preview.changes[1].changed = false;
    preview.changes[2].changed = false;
    let text = preview.render_review(false);
    assert!(text.contains(&preview.changes[0].path.to_string_lossy().to_string()));
    assert!(!text.contains(&preview.changes[1].path.to_string_lossy().to_string()));
    assert!(text.contains("保持不变的文件: 2"));
    assert!(!text.contains(preview.example));
    assert!(text.contains("实际提示符未验证"));
    let english = preview.render_compact_review_in(crate::config::ui_language::UiLanguage::English);
    assert!(english.contains("Save Prompt Style · Focus one-line"));
    assert!(english.contains("partial failures are not automatically rolled back"));
    assert!(!english.contains("creates no recovery point"));
    for change in &mut preview.changes {
        change.changed = false;
    }
    let noop = preview.render_review(false);
    assert!(noop.contains("文件已一致，无需改写"));
    assert!(!noop.contains("将更新"));
    assert!(noop.contains("样式已一致"));
    assert!(noop.contains("不新建恢复点"));
    assert!(!noop.contains("改写前创建恢复点"));
    assert!(!noop.contains("调整布局与模块外观"));
    let english = preview.render_compact_review_in(crate::config::ui_language::UiLanguage::English);
    assert!(english.contains("Style Already Matches · Focus one-line"));
    assert!(english.contains("creates no recovery point"));
    assert!(english.contains("live prompt not checked"));
    assert!(!english.contains("A recovery point precedes writes"));
    assert!(!english.contains("Changes layout and module styling"));
    assert_eq!(std::fs::read_dir(env.config_dir()).unwrap().count(), 1);
    assert_eq!(
        std::fs::read_to_string(env.managed_file("current")).unwrap(),
        "nord\n"
    );
}

#[test]
fn prompt_save_notice_distinguishes_applied_files_from_noop_and_live_appearance() {
    for style in PromptStyle::ALL {
        let applied = save_notice(style, true, false);
        assert!(applied.contains("saved."));
        assert!(applied.contains("if your shell uses these files"));
        let unchanged = save_notice(style, false, false);
        assert!(unchanged.contains("No layout files were rewritten"));
        assert!(unchanged.contains("no new recovery point"));
        assert!(!unchanged.contains("saved."));
        assert!(unchanged.contains("Live appearance was not verified"));
        let compact = save_notice(style, true, true);
        assert!(compact.contains("已保存："));
        assert!(compact.contains("若 Shell 已启用"));
        assert!(compact.contains("实际效果未检查"));
        assert!(compact.len() < applied.len());
        let noop = save_notice(style, false, true);
        assert!(noop.contains("无需修改："));
        assert!(noop.contains("未改写文件或新建恢复点"));
        assert!(!noop.contains("已保存："));
    }
}

#[test]
fn prompt_cli_accepts_all_styles_without_weakening_review_flags() {
    assert_eq!(
        PromptStyle::ALL.map(PromptStyle::id),
        ["rainbow", "minimal", "compact", "classic", "focus", "branch"]
    );
    for style in PromptStyle::ALL {
        let cli = PromptCli::try_parse_from(["prompt", style.id(), "--dry-run", "--json"]).unwrap();
        assert_eq!(cli.options.style, Some(style));
        assert!(cli.options.dry_run && cli.options.json);
        assert!(!cli.options.yes);
        let cli = PromptCli::try_parse_from(["prompt", style.id(), "--yes"]).unwrap();
        assert_eq!(cli.options.style, Some(style));
        assert!(cli.options.yes);
        assert!(PromptCli::try_parse_from(["prompt", style.id(), "--list"]).is_err());
        assert!(PromptCli::try_parse_from(["prompt", style.id(), "--dry-run", "--yes"]).is_err());
        assert!(PromptCli::try_parse_from(["prompt", style.id(), "--json"]).is_err());
    }
    assert!(PromptCli::try_parse_from(["prompt", "--dry-run"]).is_err());
    assert!(PromptCli::try_parse_from(["prompt", "--yes"]).is_err());
    assert!(PromptCli::try_parse_from(["prompt", "--list", "--json"])
        .unwrap()
        .options
        .is_catalog());
    assert!(PromptCli::try_parse_from(["prompt", "unknown-style"]).is_err());
}
