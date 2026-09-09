//! Menu-only wording. Keep CLI/JSON diagnostics and unknown errors untouched.
use crate::config::ui_language::UiLanguage;

pub(super) fn localized(text: &str, language: UiLanguage) -> &str {
    if language == UiLanguage::English {
        return text;
    }
    match text {
        "Executable found only in a fallback location, not on PATH; not launched" =>
            "找到了程序，但不在 PATH 中；未启动它。",
        "No executable detected; configuration can still be inspected" =>
            "未找到可执行程序；仍可检查配置文件。",
        "Review installation and PATH if the tool cannot be started from your shell. This check does not install anything." =>
            "若终端无法启动该工具，请检查安装和 PATH；此处不会安装。",
        "Invalid TOML; file contents and parser excerpts omitted" =>
            "TOML 语法错误；未显示文件内容或解析片段。",
        "Review the file locally before changing its settings. No repair was attempted." =>
            "请先检查该文件；尚未尝试修复。",
        "The selected file is absent; no Slate palette or layout is confirmed" =>
            "选中的配置文件不存在；无法确认 Slate 配色或布局。",
        "Review the selected path first. `slate prompt` can create Slate's standard profiles but does not install or enable Starship." =>
            "先确认路径。slate prompt 可创建标准配置，但不会安装或启用 Starship。",
        "Relative STARSHIP_CONFIG is not resolved or read; no effective file match is claimed" =>
            "STARSHIP_CONFIG 是相对路径；未解析或读取，无法确认生效文件。",
        "Review the override locally. An absolute path makes the selected file unambiguous across working directories." =>
            "请检查覆盖设置；使用绝对路径可避免工作目录变化时选中不同文件。",
        "A custom override selects a file outside Slate's two prompt write targets" =>
            "自定义覆盖选中了 Slate 不会改写的提示符文件。",
        "`slate prompt` edits standard starship.toml and Slate's plain fallback, not this override. Keep it if intentional, or review your STARSHIP_CONFIG before expecting a preset change to appear." =>
            "slate prompt 只改写标准 starship.toml 和纯文本备用配置。若此覆盖是有意设置，可保留；否则请先检查 STARSHIP_CONFIG。",
        "This file does not select the slate palette; a saved theme alone will not activate it" =>
            "此文件未选用 slate 调色板；仅保存主题不会使其生效。",
        "Presentation fields differ from the saved preset; this may be an intentional personal layout" =>
            "布局与已保存预设不同；也可能是有意保留的个人设置。",
        "Check permissions, UTF-8 encoding and the 8 MiB regular-file limit." =>
            "请检查权限、UTF-8 编码，以及文件是否为不超过 8 MiB 的普通文件。",
        "Run `slate setup` to connect this configuration." =>
            "可运行 slate setup 连接此配置。",
        _ => text,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_menu_guidance_translates_without_rewriting_unknown_errors() {
        for (original, chinese) in [
            (
                "No executable detected; configuration can still be inspected",
                "未找到可执行程序",
            ),
            (
                "Invalid TOML; file contents and parser excerpts omitted",
                "TOML 语法错误",
            ),
            (
                "A custom override selects a file outside Slate's two prompt write targets",
                "不会改写",
            ),
            (
                "Check permissions, UTF-8 encoding and the 8 MiB regular-file limit.",
                "8 MiB",
            ),
        ] {
            assert!(localized(original, UiLanguage::Chinese).contains(chinese));
            assert_eq!(localized(original, UiLanguage::English), original);
        }
        let unknown = "Cannot read /private/file: permission denied\u{1b}[2J";
        for language in [UiLanguage::Chinese, UiLanguage::English] {
            assert_eq!(localized(unknown, language), unknown);
        }
    }
}
