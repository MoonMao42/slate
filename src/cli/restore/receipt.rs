use super::output::terminal_text;
use crate::config::RestoreFileResult;

/// Failure details are data, not terminal instructions. Always identify the
/// target even when a lower layer supplies no diagnostic message.
pub(super) fn failure_text(result: &RestoreFileResult) -> String {
    format!(
        "{} · {}{}: {}",
        terminal_text(&result.display_tool),
        terminal_text(&result.original_path.to_string_lossy()),
        if result.original_path.to_str().is_none() {
            "（路径显示不完整）"
        } else {
            ""
        },
        terminal_text(
            result
                .error
                .as_deref()
                .filter(|text| !text.trim().is_empty())
                .unwrap_or("恢复失败，未提供详细原因")
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{ffi::OsString, os::unix::ffi::OsStringExt, path::PathBuf};

    #[test]
    fn failures_keep_targets_and_errors_without_interpreting_controls() {
        let mut result = RestoreFileResult {
            tool_key: "fixture".into(),
            display_tool: "工具\n\x1b[2J".into(),
            original_path: PathBuf::from("/tmp/文件\u{202e}\r.toml"),
            success: false,
            // ANSI-FIXTURE: raw input for escaping or width checks.
            error: Some("denied\x1b[32m\nFAKE SUCCESS".into()),
        };
        let text = failure_text(&result);
        assert!(text.contains("工具\\n\\u{1b}[2J"));
        assert!(text.contains("/tmp/文件\\u{202e}\\r.toml"));
        assert!(text.contains("denied\\u{1b}[32m\\nFAKE SUCCESS"));
        assert!(!text.chars().any(char::is_control));
        for error in [None, Some("  ".into())] {
            result.error = error;
            assert!(failure_text(&result).contains("未提供详细原因"));
        }
        result.original_path = PathBuf::from(OsString::from_vec(b"/tmp/\xff".to_vec()));
        assert!(failure_text(&result).contains("路径显示不完整"));
    }
}
