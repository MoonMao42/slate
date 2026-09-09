//! Compare illustrations before preparing any personal configuration changes.
use super::*;

enum Next {
    ChooseAnother,
    Leave,
}

// Interactive guidance only; CLI identifiers and machine-readable descriptions
// remain stable for scripts and exported catalogs.
fn style_hint(style: PromptStyle) -> &'static str {
    style_hint_in(style, crate::cli::ui_language::current())
}

pub(super) fn style_hint_in(
    style: PromptStyle,
    language: crate::config::ui_language::UiLanguage,
) -> &'static str {
    let tr = |zh, en| crate::config::ui_language::Text { zh, en }.get(language);
    match style {
        PromptStyle::Rainbow => tr(
            "用户、目录、分支和时钟；推荐图标字体",
            "User, directory, branch and clock; icon font recommended",
        ),
        PromptStyle::Minimal => tr(
            "目录与 Git 状态在上，下一行输入；含命令耗时",
            "Directory and Git above the input; includes duration",
        ),
        PromptStyle::Compact => tr(
            "目录、分支和 Git 状态；节省纵向空间",
            "Directory, branch and Git status on one line",
        ),
        PromptStyle::Classic => tr(
            "用户、目录与 Git；默认仅 SSH 显示主机",
            "User, directory and Git; host shown for SSH",
        ),
        PromptStyle::Focus => tr(
            "只留目录和输入符号，不显示 Git 或时钟",
            "Directory and input only; no Git or clock",
        ),
        PromptStyle::Branch => tr(
            "目录和分支，不显示 Git 改动统计或耗时",
            "Directory and branch; no Git statistics or duration",
        ),
    }
}

#[derive(Debug, PartialEq, Eq)]
enum SavedLayout {
    Known(PromptStyle),
    Unspecified,
    Unreadable,
}

impl SavedLayout {
    fn initial_preview(&self, last_preview: Option<PromptStyle>) -> Option<PromptStyle> {
        last_preview.or(match self {
            Self::Known(style) => Some(*style),
            Self::Unspecified | Self::Unreadable => None,
        })
    }

    fn read(env: &SlateEnv) -> Self {
        match crate::config::prompt::saved_style(env) {
            Ok(Some(style)) => Self::Known(style),
            Ok(None) => Self::Unspecified,
            Err(_) => Self::Unreadable,
        }
    }

    fn summary(&self) -> String {
        match self {
            Self::Known(style) => {
                format!(
                    "{}{} · {}",
                    tr("已保存：", "Saved: "),
                    menu_style_label(*style),
                    tr("实际提示符未检查", "Live prompt not checked")
                )
            }
            Self::Unspecified => tr(
                "尚未保存样式 · 保留现有自定义提示符，不假定默认样式",
                "No saved style; keeping your custom prompt",
            )
            .into(),
            Self::Unreadable => tr(
                "无法读取已保存样式 · 仍可浏览示例，不假定默认样式",
                "Saved style unreadable; examples remain available",
            )
            .into(),
        }
    }

    fn label(&self, style: PromptStyle) -> String {
        if *self == Self::Known(style) {
            format!("{} · {}", menu_style_label(style), tr("已保存", "Saved"))
        } else {
            menu_style_label(style).to_owned()
        }
    }
}

pub(super) fn handle(env: &SlateEnv) -> Result<()> {
    if !interactive() {
        return print_catalog(false);
    }
    let mut page = None;
    let result = handle_pages(env, &mut page);
    match page {
        Some(page) => page.finish(result),
        None => result,
    }
}

fn clear_browser(page: &mut Option<crate::cli::menu::ReadOnlyPage>) -> Result<()> {
    if page.is_none() {
        *page = Some(crate::cli::menu::ReadOnlyPage::enter()?);
    }
    if let Some(page) = page {
        page.clear()?;
    }
    Ok(())
}

fn handle_pages(env: &SlateEnv, page: &mut Option<crate::cli::menu::ReadOnlyPage>) -> Result<()> {
    let mut last_preview = None;
    loop {
        clear_browser(page)?;
        let saved = SavedLayout::read(env);
        super::super::file_output::write_output(&format!("\n  {}\n\n", saved.summary()))?;
        let mut menu = crate::cli::menu::select(tr("选择提示符样式", "Choose Prompt Style"))
            .escape_value(None);
        if let Some(style) = saved.initial_preview(last_preview) {
            menu = menu.initial_value(Some(style));
        }
        for style in PromptStyle::ALL {
            menu = menu.item(Some(style), saved.label(style), style_hint(style));
        }
        let Some(style) = menu
            .item(None, tr("返回上级", "Back"), "")
            .interact()
            .map_err(input_error)?
        else {
            return Ok(());
        };
        // Navigation state only; viewing an example is not a saved preference.
        last_preview = Some(style);
        if matches!(browse_style(env, style, page)?, Next::Leave) {
            return Ok(());
        }
    }
}

fn browse_style(
    env: &SlateEnv,
    style: PromptStyle,
    page: &mut Option<crate::cli::menu::ReadOnlyPage>,
) -> Result<Next> {
    let mut last_read_only_action = None;
    loop {
        clear_browser(page)?;
        let saved = SavedLayout::read(env);
        super::super::file_output::write_output(&format!(
            "\n  {}\n\n  {} · {}\n\n{}\n\n  {}\n",
            saved.summary(),
            menu_style_label(style),
            style_hint(style),
            style.sample(),
            tr(
                "仅样式示意，并非实时提示符；浏览不改文件、不执行命令。",
                "Illustration only; browsing changes no files and runs no commands."
            )
        ))?;
        let mut menu =
            crate::cli::menu::select(tr("样式预览", "Style Preview")).escape_value("another");
        if let Some(action) = last_read_only_action {
            menu = menu.initial_value(action);
        }
        match PreparedPrompt::has_known_theme(env) {
            Ok(true) => {
                menu = menu.item(
                    "review",
                    tr("查看改动并确认", "Review Changes"),
                    tr(
                        "先查看要修改的文件，再决定是否保存",
                        "Review files before saving",
                    ),
                );
            }
            Ok(false) => {
                super::super::file_output::write_output(
                    tr("\n  尚未选定可用主题：先保存主题，才能应用提示符样式。\n  现在仍可比较示例，不会自动选择默认主题。\n\n", "\n  Save a recognized theme before applying a prompt style.\n  You can still browse; no default theme will be selected.\n\n"),
                )?;
                menu = menu.item(
                    "theme",
                    tr("先选择主题", "Choose a Theme First"),
                    tr(
                        "另行确认 · 会影响检测到的工具，不只提示符",
                        "Separate confirmation; affects detected tools too",
                    ),
                );
            }
            Err(_) => {
                // Unknown/unsafe input is not an absent theme. Never advertise
                // a replacement action on the strength of a failed read.
                cliclack::log::warning(
                    tr("已保存主题无法安全读取，请先运行 slate status 检查。仍可浏览示例，不会替换主题。", "Saved theme unreadable. Run slate status; browsing will not replace it."),
                )?;
            }
        }
        let action = menu
            .item("another", tr("返回样式列表", "Back to Styles"), "")
            .item("refresh", tr("刷新已保存状态", "Refresh Saved State"), "")
            .item(
                "check",
                tr("检查提示符配置", "Check Prompt Configuration"),
                "",
            )
            .item("back", tr("返回上级", "Back"), "")
            .interact()
            .map_err(input_error)?;
        // Keep read-only inspection convenient without preselecting a write
        // or theme handoff after returning from another action.
        last_read_only_action = matches!(action, "check" | "refresh").then_some(action);
        // Leave the browser before reviews or another page takes ownership.
        // Never erase a write result or nest another alternate screen.
        if matches!(action, "review" | "theme" | "check") {
            if let Some(page) = page.take() {
                page.finish(Ok::<(), SlateError>(()))?;
            }
        }
        match action {
            "review" => {
                // Capture afresh, not when the page was opened. Apply retains
                // the existing under-lock verification, checkpoint and consent.
                let result = PreparedPrompt::capture(env, style)
                    .and_then(|plan| review_and_apply(plan, false));
                if action_result(result, |message| cliclack::log::warning(message))? == Some(true) {
                    return Ok(Next::Leave);
                }
            }
            "theme" => {
                let result = super::super::theme_handoff::open(
                    env,
                    super::super::theme_handoff::Origin::PromptLayout,
                );
                action_result(result, |message| cliclack::log::warning(message))?;
                // Return to this exact style, whether the user saved or
                // canceled the theme picker, then re-read its prerequisite.
            }
            "another" => return Ok(Next::ChooseAnother),
            "check" => {
                let result = super::super::doctor::show_tool_files(
                    env,
                    "starship",
                    tr("返回样式预览", "Back to Preview"),
                );
                action_result(result, |message| cliclack::log::warning(message))?;
            }
            "refresh" => {}
            _ => return Ok(Next::Leave),
        }
    }
}

/// Only the interactive browser recovers; explicit commands retain their errors.
/// Terminal failures and cancellation must escape rather than loop on dead input.
fn action_result<T>(
    result: Result<T>,
    warn: impl FnOnce(&str) -> std::io::Result<()>,
) -> Result<Option<T>> {
    match result {
        Ok(value) => Ok(Some(value)),
        Err(error @ (SlateError::UserCancelled | SlateError::IOError(_))) => Err(error),
        Err(error) => {
            warn(&action_warning(&error, crate::cli::ui_language::current()))?;
            Ok(None)
        }
    }
}

fn action_warning(error: &SlateError, language: crate::config::ui_language::UiLanguage) -> String {
    let error = super::super::file_output::terminal_text(&error.to_string());
    let text = |zh, en| crate::config::ui_language::Text { zh, en }.get(language);
    format!(
        "{}{error}\n{}\n{}slate restore --list\n{}slate recover --dry-run",
        text("操作已停止：", "Action stopped: "),
        text(
            "返回样式预览，不自动重试；此前的部分改动可能保留，请先检查。",
            "Returning to preview without retrying; earlier changes may remain. Inspect before retrying."
        ),
        text("文件恢复：", "File recovery: "),
        text("未完成的预览：", "Unfinished preview: "),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompt_initial_selection_prefers_browsing_then_saved_without_inventing_a_preference() {
        for style in PromptStyle::ALL {
            let saved = SavedLayout::Known(style);
            assert_eq!(saved.initial_preview(None), Some(style));
            for preview in PromptStyle::ALL {
                assert_eq!(saved.initial_preview(Some(preview)), Some(preview));
            }
            assert_eq!(
                SavedLayout::Unspecified.initial_preview(Some(style)),
                Some(style)
            );
            assert_eq!(
                SavedLayout::Unreadable.initial_preview(Some(style)),
                Some(style)
            );
        }
        assert_eq!(SavedLayout::Unspecified.initial_preview(None), None);
        assert_eq!(SavedLayout::Unreadable.initial_preview(None), None);
    }

    #[test]
    #[ignore = "private PTY fixture; invoked by tests/support/prompt_menu_pty.py"]
    fn prompt_menu_pty_fixture() {
        let root =
            std::env::var_os("SLATE_PROMPT_PTY_ROOT").expect("explicit private fixture root");
        let env = SlateEnv::from_vars(|name| (name == "SLATE_HOME").then(|| root.clone())).unwrap();
        assert!(interactive(), "fixture requires a private PTY");
        handle(&env).unwrap();
    }

    #[test]
    fn prompt_menu_saved_layout_refreshes_without_assuming_or_writing_defaults() {
        let root = tempfile::tempdir().unwrap();
        let env = SlateEnv::with_home(root.path().to_owned());
        let path = env.managed_file("config.toml");
        let missing = SavedLayout::read(&env);
        assert_eq!(missing, SavedLayout::Unspecified);
        assert!(missing.summary().contains("尚未保存样式"));
        assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        for style in PromptStyle::ALL {
            let content = format!("# personal\n[prompt]\nstyle = '{}'\n", style.id());
            std::fs::write(&path, &content).unwrap();
            let saved = SavedLayout::read(&env);
            assert_eq!(saved, SavedLayout::Known(style));
            assert!(saved.summary().contains("实际提示符未检查"));
            for candidate in PromptStyle::ALL {
                assert_eq!(
                    saved.label(candidate).contains("已保存"),
                    candidate == style
                );
            }
            assert_eq!(std::fs::read_to_string(&path).unwrap(), content);
        }
        for content in [
            "[broken",
            "[prompt]\nstyle = 'unknown'",
            "[prompt]\nstyle = 1",
        ] {
            std::fs::write(&path, content).unwrap();
            let saved = SavedLayout::read(&env);
            assert_eq!(saved, SavedLayout::Unreadable);
            assert!(saved.summary().contains("无法读取已保存样式"));
            assert!(PromptStyle::ALL
                .into_iter()
                .all(|style| !saved.label(style).contains("已保存")));
            assert_eq!(std::fs::read_to_string(&path).unwrap(), content);
        }
        assert_eq!(
            std::fs::read_dir(path.parent().unwrap()).unwrap().count(),
            1
        );
    }

    #[test]
    fn prompt_menu_failure_retains_browsing_without_changing_unreadable_state() {
        let root = tempfile::tempdir().unwrap();
        let env = SlateEnv::with_home(root.path().to_owned());
        let path = env.managed_file("current");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, b"nord\n").unwrap();
        let preferences = env.managed_file("config.toml");
        std::fs::write(&preferences, b"[broken").unwrap();
        let mut notices = Vec::new();
        let result = action_result(
            PreparedPrompt::capture(&env, PromptStyle::Focus),
            |message| {
                notices.push(message.to_owned());
                Ok(())
            },
        )
        .unwrap();
        assert!(result.is_none());
        assert_eq!(notices.len(), 1);
        assert!(notices[0].contains("不自动重试"));
        assert_eq!(std::fs::read(&path).unwrap(), b"nord\n");
        assert_eq!(std::fs::read(&preferences).unwrap(), b"[broken");
        assert_eq!(
            std::fs::read_dir(path.parent().unwrap()).unwrap().count(),
            2
        );
    }

    #[test]
    fn prompt_menu_preserves_success_decline_cancel_and_terminal_failures() {
        for value in [true, false] {
            assert_eq!(
                action_result(Ok(value), |_| panic!("unexpected warning")).unwrap(),
                Some(value)
            );
        }
        assert!(matches!(
            action_result::<()>(Err(SlateError::UserCancelled), |_| panic!(
                "unexpected warning"
            )),
            Err(SlateError::UserCancelled)
        ));
        assert!(matches!(
            action_result::<()>(
                Err(std::io::Error::from(std::io::ErrorKind::BrokenPipe).into()),
                |_| panic!("unexpected warning")
            ),
            Err(SlateError::IOError(_))
        ));
        assert!(matches!(
            action_result::<()>(Err(SlateError::ConfigurationBusy), |_| Err(
                std::io::ErrorKind::BrokenPipe.into()
            )),
            Err(SlateError::IOError(_))
        ));
    }

    #[test]
    fn prompt_menu_failure_escapes_untrusted_error_text() {
        let result = action_result::<()>(
            Err(SlateError::InvalidConfig("bad\x1b[2J\rpath".into())),
            |message| {
                assert!(!message.contains('\x1b'));
                assert!(!message.contains('\r'));
                assert!(message.contains("此前的部分改动可能保留"));
                Ok(())
            },
        )
        .unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn prompt_failure_guidance_is_bilingual_compact_and_keeps_recovery_commands() {
        use crate::config::ui_language::UiLanguage;
        let error = SlateError::InvalidConfig("fixture\x1b[2J\rpath".into());
        for (language, title, safety) in [
            (
                UiLanguage::Chinese,
                "操作已停止：",
                "此前的部分改动可能保留",
            ),
            (
                UiLanguage::English,
                "Action stopped: ",
                "earlier changes may remain",
            ),
        ] {
            let message = action_warning(&error, language);
            assert!(message.starts_with(title));
            assert!(message.contains(safety));
            assert!(message.contains("fixture"));
            assert!(!message.contains('\x1b') && !message.contains('\r'));
            assert!(message.contains("slate restore --list"));
            assert!(message.contains("slate recover --dry-run"));
            assert_eq!(message.lines().count(), 4);
        }
    }
}
