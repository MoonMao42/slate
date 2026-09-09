//! Review-first prompt styles. No native prompt/custom-command preview execution.
use super::ui_language::tr;
use crate::{
    config::prompt::{PreparedPrompt, PromptPreview, PromptStyle},
    env::SlateEnv,
    error::{Result, SlateError},
};
use clap::Args;
use serde::Serialize;
use std::{fmt::Write, io::IsTerminal};

mod menu;

/// Shared interactive names; CLI catalogs and serialized identifiers stay stable.
pub(super) fn menu_style_label(style: PromptStyle) -> &'static str {
    style_label_in(style, super::ui_language::current())
}

fn style_label_in(
    style: PromptStyle,
    language: crate::config::ui_language::UiLanguage,
) -> &'static str {
    let zh = match style {
        PromptStyle::Rainbow => "彩虹分段",
        PromptStyle::Minimal => "简洁双行",
        PromptStyle::Compact => "紧凑单行",
        PromptStyle::Classic => "经典双行",
        PromptStyle::Focus => "专注单行",
        PromptStyle::Branch => "分支单行",
    };
    crate::config::ui_language::Text {
        zh,
        en: style.label(),
    }
    .get(language)
}

#[cfg(test)]
mod tests;

#[derive(Args)]
#[command(group(clap::ArgGroup::new("prompt_inspection").args(["list", "dry_run"]).multiple(false)))]
pub struct PromptOptions {
    /// Layout preset; omit for an interactive choice
    #[arg(value_enum, conflicts_with = "list")]
    style: Option<PromptStyle>,
    /// List built-in layouts without reading your profile or starting tools
    #[arg(long)]
    list: bool,
    /// Language for the text catalog; does not read or change preferences
    #[arg(long, requires = "list", conflicts_with_all = ["json", "style", "yes", "dry_run"], value_parser = ["zh-CN", "en"])]
    language: Option<String>,
    /// Show an illustrative sample and exact file-change scope without writing
    #[arg(long, requires = "style", conflicts_with = "yes")]
    dry_run: bool,
    /// Confirm a preset without an interactive prompt
    #[arg(long, requires = "style")]
    yes: bool,
    /// Emit a read-only catalog or change plan as JSON
    #[arg(long, requires = "prompt_inspection")]
    json: bool,
}

impl PromptOptions {
    pub fn is_catalog(&self) -> bool {
        self.list
    }
}

pub fn print_catalog(json: bool) -> Result<()> {
    #[derive(Serialize)]
    struct Choice {
        id: PromptStyle,
        label: &'static str,
        description: &'static str,
        example: &'static str,
    }
    let choices: Vec<_> = PromptStyle::ALL
        .into_iter()
        .map(|style| Choice {
            id: style,
            label: style.label(),
            description: style.description(),
            example: style.sample(),
        })
        .collect();
    if json {
        return super::file_output::write_output(&format!(
            "{}\n",
            serde_json::to_string_pretty(
                &serde_json::json!({"schema_version": 1, "styles": choices})
            )?
        ));
    }
    super::file_output::write_output(&catalog_text(
        crate::config::ui_language::UiLanguage::English,
    ))
}

fn catalog_text(language: crate::config::ui_language::UiLanguage) -> String {
    use crate::config::ui_language::{Text, UiLanguage};
    let mut output = Text {
        zh: "提示符样式 · 仅为示意，非实时预览\n\n",
        en: "Prompt layouts · illustrations, not live previews\n\n",
    }
    .get(language)
    .to_owned();
    for style in PromptStyle::ALL {
        let _ = writeln!(
            output,
            "{} — {}\n{}\n{}\n",
            style.id(),
            style_label_in(style, language),
            if language == UiLanguage::Chinese {
                menu::style_hint_in(style, language)
            } else {
                style.description()
            },
            style.sample()
        );
    }
    output.push_str(
        Text {
            zh: "只读查看改动：slate prompt minimal --dry-run\n",
            en: "Review without changes: slate prompt minimal --dry-run\n",
        }
        .get(language),
    );
    output
}

pub fn print_catalog_options(options: &PromptOptions) -> Result<()> {
    if options.language.as_deref() == Some("zh-CN") {
        return super::file_output::write_output(&catalog_text(
            crate::config::ui_language::UiLanguage::Chinese,
        ));
    }
    print_catalog(options.json)
}

pub fn handle(env: &SlateEnv, options: &PromptOptions) -> Result<()> {
    if options.list {
        return print_catalog_options(options);
    }
    let Some(style) = options.style else {
        return handle_menu(env);
    };
    apply_choice(env, style, options.dry_run, options.yes, options.json)
}

pub(super) fn handle_menu(env: &SlateEnv) -> Result<()> {
    menu::handle(env)
}

fn interactive() -> bool {
    std::io::stdin().is_terminal() && std::io::stdout().is_terminal()
}
fn input_error(error: std::io::Error) -> SlateError {
    if error.kind() == std::io::ErrorKind::Interrupted {
        SlateError::UserCancelled
    } else {
        SlateError::IOError(error)
    }
}

impl PromptPreview {
    fn render(&self) -> String {
        self.render_review(true)
    }

    fn render_review(&self, detailed: bool) -> String {
        if !detailed {
            return self.render_compact_review();
        }
        let mut output = format!(
            "{} · {}\nIllustration:\n{}\n\n",
            self.style.label(),
            self.theme,
            self.example
        );
        if let Some(selected) = &self.starship_config_override {
            let _ = writeln!(
                output,
                "Captured STARSHIP_CONFIG: {}{}\n",
                super::file_output::terminal_text(&selected.path),
                if selected.path_is_lossy {
                    " (lossy display; not a copyable path)"
                } else {
                    ""
                },
            );
            output.push_str("Only the file targets listed below are updated. This override may select another file; use `slate doctor starship` to inspect it. The live prompt was not checked.\n\n");
        }
        for change in &self.changes {
            let _ = writeln!(
                output,
                "{} {}",
                if change.changed { "Update" } else { "Keep" },
                super::file_output::terminal_text(&change.path.to_string_lossy())
            );
        }
        for note in &self.notes {
            let _ = writeln!(output, "• {note}");
        }
        output
    }

    fn render_compact_review(&self) -> String {
        self.render_compact_review_in(super::ui_language::current())
    }

    fn render_compact_review_in(&self, language: crate::config::ui_language::UiLanguage) -> String {
        let tr = |zh, en| crate::config::ui_language::Text { zh, en }.get(language);
        let changed = self.changes.iter().filter(|change| change.changed).count();
        let mut output = format!(
            "\n  {} · {} · {}\n\n",
            if changed == 0 {
                tr("样式已一致", "Style Already Matches")
            } else {
                tr("保存提示符样式", "Save Prompt Style")
            },
            style_label_in(self.style, language),
            self.theme
        );
        if changed == 0 {
            output.push_str(tr("  文件已一致，无需改写。\n", "  Files already match.\n"));
        } else {
            output.push_str(tr("  将更新：\n", "  Files to update:\n"));
            for change in self.changes.iter().filter(|change| change.changed) {
                let _ = writeln!(
                    output,
                    "    {}",
                    super::file_output::terminal_text(&change.path.to_string_lossy())
                );
            }
        }
        let kept = self.changes.len() - changed;
        if kept > 0 {
            let _ = writeln!(
                output,
                "  {}: {kept}",
                tr("保持不变的文件", "Unchanged files")
            );
        }
        if changed > 0 {
            output.push_str(tr("\n  调整布局与模块外观，保留自定义命令定义。\n  不改变主题、字体或 Shell 启用设置；实际提示符未验证。\n", "\n  Changes layout and module styling; keeps custom command definitions.\n  Theme, font and shell activation stay unchanged; live prompt not checked.\n"));
        } else {
            output.push_str(tr("  当前方案不改文件、不新建恢复点；实际提示符未验证。\n", "  This plan changes no files and creates no recovery point; live prompt not checked.\n"));
        }
        if let Some(selected) = &self.starship_config_override {
            let _ = writeln!(
                output,
                "  STARSHIP_CONFIG：{}{}",
                super::file_output::terminal_text(&selected.path),
                if selected.path_is_lossy {
                    tr(
                        "（路径显示有损，不可直接复制）",
                        " (lossy path; do not copy)",
                    )
                } else {
                    ""
                }
            );
            output.push_str(tr(
                "  此变量可能指向其他文件；本次只更新上面列出的文件。\n",
                "  This override may select another file; only listed targets are updated.\n",
            ));
        }
        if changed > 0 {
            output.push_str(tr("  改写前创建恢复点；部分写入失败不会自动回滚。\n", "  A recovery point precedes writes; partial failures are not automatically rolled back.\n"));
        }
        let _ = writeln!(
            output,
            "  {}slate prompt {} --dry-run\n",
            tr("完整明细：", "Full review: "),
            self.style.id()
        );
        output
    }
}

fn apply_choice(
    env: &SlateEnv,
    style: PromptStyle,
    dry_run: bool,
    yes: bool,
    json: bool,
) -> Result<()> {
    let plan = PreparedPrompt::capture(env, style)?;
    let preview = plan.preview();
    if json {
        return super::file_output::write_output(&format!(
            "{}\n",
            serde_json::to_string_pretty(&preview)?
        ));
    }
    if dry_run {
        return super::file_output::write_output(&format!(
            "{}No files or processes were changed.\n",
            preview.render()
        ));
    }
    review_and_apply(plan, yes).map(|_| ())
}

/// True only after confirmed application; declining lets the browser continue.
fn review_and_apply(plan: PreparedPrompt, yes: bool) -> Result<bool> {
    super::file_output::write_required(&plan.preview().render_review(!interactive()))?;
    if !yes {
        if !interactive() {
            return Err(SlateError::InvalidConfig("Non-interactive prompt changes require --yes; review with --dry-run first. Nothing was changed.".into()));
        }
        if !super::menu::select(tr("保存这个提示符样式？", "Save this prompt style?"))
            .escape_value(false)
            .initial_value(false)
            .item(false, tr("暂不保存", "Cancel"), "")
            .item(
                true,
                tr("确认保存", "Save"),
                tr("写入上面列出的修改", "Write the reviewed changes"),
            )
            .interact()
            .map_err(input_error)?
        {
            return Ok(false);
        }
    }
    let restore_point = plan.apply()?;
    let compact = interactive();
    if let Some(id) = &restore_point {
        let recovery = if compact {
            format!(
                "  {}slate restore {id} --dry-run\n",
                tr("查看恢复方案：", "Review recovery: ")
            )
        } else {
            format!(
                "Restore point: {id}\nRecover the previous layout: slate restore {id} --dry-run\n"
            )
        };
        std::io::Write::write_all(&mut std::io::stderr().lock(), recovery.as_bytes())?;
    }
    super::file_output::write_required(&save_notice(plan.style, restore_point.is_some(), compact))?;
    Ok(true)
}

fn save_notice(style: PromptStyle, changed: bool, compact: bool) -> String {
    if compact {
        if super::ui_language::current() == crate::config::ui_language::UiLanguage::English {
            return if changed {
                format!("\n  Saved: {}\n  Your next prompt loads it if Starship uses these files; live appearance not checked.\n", style.label())
            } else {
                format!("\n  No changes: {}\n  Files already match; no new recovery point. Live appearance not checked.\n", style.label())
            };
        }
        return if changed {
            format!("\n  已保存：{}\n  若 Shell 已启用这份 Starship 配置，下次提示符会读取；实际效果未检查。\n", menu_style_label(style))
        } else {
            format!(
                "\n  无需修改：{}\n  文件已一致，未改写文件或新建恢复点；实际效果未检查。\n",
                menu_style_label(style)
            )
        };
    }
    if changed {
        format!(
            "{} saved. Starship loads the files at its next prompt if your shell uses these files. Shell activation and your theme were not changed; live appearance was not verified.\n",
            style.label()
        )
    } else {
        format!(
            "{} already matches the reviewed files. No layout files were rewritten and no new recovery point was created. Live appearance was not verified.\n",
            style.label()
        )
    }
}
