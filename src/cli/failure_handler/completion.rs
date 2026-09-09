use super::{ExecutionSummary, InstallStatus, SkipReason, ToolApplyStatus};
use crate::cli::file_output::terminal_text;
use crate::config::ui_language::{Text, UiLanguage};
use crate::detection::{TerminalKind, TerminalProfile};

impl ExecutionSummary {
    /// Interactive receipt: compact success, complete actionable failure details.
    /// Rendering never probes native tools or changes the recorded outcome.
    pub(super) fn compact_completion(
        &self,
        terminal: &TerminalProfile,
        language: UiLanguage,
    ) -> String {
        let tr = |zh, en| Text { zh, en }.get(language);
        let mut lines = vec![if self.is_successful() {
            tr("✦ 设置完成", "✦ Setup complete").to_owned()
        } else {
            tr("✦ 设置尚未全部完成", "✦ Setup incomplete").to_owned()
        }];
        if !self.tool_results.is_empty() {
            lines.push(format!(
                "{} {} · {} {}",
                tr("已安装", "Installed"),
                self.success_count(),
                tr("失败", "Failed"),
                self.failure_count()
            ));
        }
        if !self.theme_results.is_empty() {
            lines.push(format!(
                "{} {} · {} {} · {} {}",
                tr("配色已更新", "Configurations updated"),
                self.configured_count(),
                tr("失败", "Failed"),
                self.theme_failure_count(),
                tr("缺少接入配置", "Missing integration"),
                self.missing_integration_skip_count()
            ));
        }
        if !self.theme_applied {
            lines.push(
                tr(
                    "主题或 Shell 设置未完成，之前的文件改动可能仍保留。",
                    "Theme/shell setup did not finish; earlier file changes may remain.",
                )
                .into(),
            );
        }
        if self.font_requested || self.font_applied {
            lines.push(
                if self.font_applied && self.font_available {
                    tr(
                        "字体选择已保存；未验证实际显示。",
                        "Font choice saved; rendering was not verified.",
                    )
                } else if self.font_available {
                    tr(
                        "字体可用，但选择尚未保存。",
                        "Font available, but its choice was not saved.",
                    )
                } else {
                    tr(
                        "所选字体尚未找到或安装。",
                        "Selected font was not found or installed.",
                    )
                }
                .into(),
            );
        }
        for tool in &self.tool_results {
            if tool.status == InstallStatus::Failed {
                lines.push(format!(
                    "✗ {}: {}",
                    terminal_text(&tool.tool_label),
                    terminal_text(
                        tool.error_message
                            .as_deref()
                            .unwrap_or(tr("安装失败", "Installation failed"))
                    )
                ));
                // Do not turn an arbitrary label into an executable retry command.
                if crate::cli::setup::validate_retry_tool(&tool.tool_id).is_ok() {
                    lines.push(format!("  slate setup --only {}", tool.tool_id));
                }
            }
        }
        for result in &self.theme_results {
            match &result.status {
                ToolApplyStatus::Failed(error) => lines.push(format!(
                    "✗ {}: {}",
                    terminal_text(&result.tool_name),
                    terminal_text(&error.to_string())
                )),
                ToolApplyStatus::Skipped(SkipReason::MissingIntegrationConfig) => {
                    lines.push(format!(
                        "! {}: {}",
                        terminal_text(&result.tool_name),
                        tr(
                            "缺少接入配置，请在工具配色中检查。",
                            "Missing integration; check this tool in Connect Tools."
                        )
                    ))
                }
                _ => {}
            }
        }
        for message in self.issues.iter().chain(&self.notices) {
            lines.push(format!("• {}", terminal_text(message)));
        }
        if !self.is_successful() {
            lines.push(
                tr(
                    "未自动回滚；恢复方法见本次恢复点提示。",
                    "No automatic rollback; use this run's recovery-point instructions.",
                )
                .into(),
            );
        }
        lines.push(String::new());
        if !terminal.session().can_reload_terminal() {
            lines.push(if terminal.session().is_remote() {
                tr("本次仅配置远端主机；客户端字体和透明度需在本地设置。", "Only this remote host was configured; set client font and opacity locally.")
            } else {
                tr("本次使用隔离配置，不代表主机终端窗口已改变。", "This isolated profile does not establish changes to host terminal windows.")
            }.into());
        } else {
            lines.push(tr("新开终端标签页以加载 Shell 设置；未验证窗口实际外观。", "Open a fresh tab to load shell changes; live window appearance was not verified.").into());
            match terminal.kind() {
                TerminalKind::TerminalApp => lines.push(tr("Terminal.app 字体需在设置 → 描述文件 → 文本中选择；不支持 Slate 磨砂效果。", "Choose the Terminal.app font in Settings > Profiles > Text; Slate frost is unsupported.").into()),
                TerminalKind::Kitty | TerminalKind::Alacritty => lines.push(tr("配色未更新时请新开窗口；此终端不支持 Slate 磨砂效果。", "Open a new window if colors did not reload; Slate frost is unsupported here.").into()),
                TerminalKind::Ghostty => lines.push(tr("字体或窗口外观未更新时，再重启 Ghostty。", "Restart Ghostty only if font or window visuals remain unchanged.").into()),
                TerminalKind::Unknown => lines.push(tr("字体、透明度等效果取决于当前终端支持。", "Font and opacity effects depend on your terminal's support.").into()),
            }
        }
        lines.join("\n") + "\n"
    }
}

#[cfg(test)]
mod tests {
    use super::super::ToolInstallResult;
    use super::*;

    #[test]
    #[ignore = "private PTY fixture; renders retained results without running setup"]
    fn completion_terminal_fixture() {
        let env = crate::env::SlateEnv::from_process().unwrap();
        assert!(env.session().is_isolated());
        crate::cli::ui_language::load_saved_ui_language(&env).unwrap();
        let mut summary = ExecutionSummary::new();
        summary.theme_applied = true;
        match std::env::var("SLATE_RECEIPT_CASE").unwrap().as_str() {
            "success" => {}
            "failure" => summary.add_tool_result(ToolInstallResult {
                tool_id: "bat".into(),
                tool_label: "bat".into(),
                status: InstallStatus::Failed,
                error_message: Some("fixture download failed".into()),
            }),
            _ => panic!("unknown fixture"),
        }
        let terminal = TerminalProfile::from_env_vars(Some("ghostty"), None)
            .with_session(crate::session::SessionContext::isolated());
        eprintln!(
            "RECEIPT-BEGIN\n{}RECEIPT-END",
            summary.format_completion_message_for_terminal(&terminal)
        );
        assert_eq!(std::fs::read_dir(env.home()).unwrap().count(), 0);
    }

    #[test]
    fn compact_receipt_never_hides_missing_font_or_adapter_failures() {
        let terminal = TerminalProfile::from_env_vars(None, None);
        for language in [UiLanguage::Chinese, UiLanguage::English] {
            for case in ["theme", "font", "font-unsaved", "adapter", "missing"] {
                let mut summary = ExecutionSummary::new();
                summary.theme_applied = case != "theme";
                summary.overall_success = true; // A stale mirror must not decide the receipt.
                if case.starts_with("font") {
                    summary.font_requested = true;
                    summary.font_available = case == "font-unsaved";
                }
                if matches!(case, "adapter" | "missing") {
                    summary.set_theme_results(vec![crate::adapter::ToolApplyResult {
                        tool_name: "ghostty".into(),
                        requires_new_shell: false,
                        status: if case == "adapter" {
                            ToolApplyStatus::Failed(crate::error::SlateError::Internal(
                                "fixture write failed".into(),
                            ))
                        } else {
                            ToolApplyStatus::Skipped(SkipReason::MissingIntegrationConfig)
                        },
                    }]);
                }
                let text = summary.compact_completion(&terminal, language);
                assert!(
                    text.contains(if language == UiLanguage::Chinese {
                        "尚未全部完成"
                    } else {
                        "Setup incomplete"
                    }),
                    "{case}: {text}"
                );
                if case == "adapter" {
                    assert!(text.contains("fixture write failed"));
                }
                if case == "missing" {
                    assert!(text.contains("ghostty"));
                }
            }
        }
    }

    #[test]
    fn compact_file_only_receipt_does_not_recommend_host_restarts() {
        for session in [
            crate::session::SessionContext::isolated(),
            crate::session::SessionContext::from_vars(|key| {
                (key == "SSH_CONNECTION").then(|| "private".into())
            }),
        ] {
            let terminal =
                TerminalProfile::from_env_vars(Some("ghostty"), None).with_session(session);
            let mut summary = ExecutionSummary::new();
            summary.theme_applied = true;
            for language in [UiLanguage::Chinese, UiLanguage::English] {
                let text = summary.compact_completion(&terminal, language);
                assert!(!text.contains("Restart Ghostty"));
                assert!(!text.contains("重启 Ghostty"));
                assert!(text.contains(if terminal.session().is_remote() {
                    if language == UiLanguage::Chinese {
                        "远端主机"
                    } else {
                        "remote host"
                    }
                } else if language == UiLanguage::Chinese {
                    "隔离配置"
                } else {
                    "isolated profile"
                }));
            }
        }
    }

    #[test]
    fn compact_receipt_is_bilingual_and_retains_failures_without_terminal_controls() {
        let terminal = TerminalProfile::from_env_vars(Some("ghostty"), None);
        for language in [UiLanguage::Chinese, UiLanguage::English] {
            let mut summary = ExecutionSummary::new();
            summary.theme_applied = true;
            let success = summary.compact_completion(&terminal, language);
            assert!(success.lines().count() < 8, "{success}");
            assert!(success.contains(if language == UiLanguage::Chinese {
                "设置完成"
            } else {
                "Setup complete"
            }));
            summary.add_tool_result(ToolInstallResult {
                tool_id: "bat".into(),
                tool_label: "bat\x1b[2J".into(),
                status: InstallStatus::Failed,
                error_message: Some("download failed\rhidden".into()),
            });
            summary.add_issue("permission denied");
            let failure = summary.compact_completion(&terminal, language);
            assert!(!failure.contains('\x1b'));
            assert!(!failure.contains('\r'));
            assert!(failure.contains("download failed"));
            assert!(failure.contains("permission denied"));
            assert!(failure.contains("slate setup --only bat"));
            assert!(failure.contains(if language == UiLanguage::Chinese {
                "未自动回滚"
            } else {
                "No automatic rollback"
            }));
            assert!(!summary.is_successful());
        }
    }
}
