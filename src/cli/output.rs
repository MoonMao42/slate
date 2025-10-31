use crate::ThemeError;
use ansi_term::Colour;

/// Fixed color scheme for CLI output (not theme-dependent per)
pub struct ColorScheme;

impl ColorScheme {
    pub fn success() -> ansi_term::Colour {
        Colour::Green
    }

    pub fn failure() -> ansi_term::Colour {
        Colour::Red
    }

    pub fn warning() -> ansi_term::Colour {
        Colour::Yellow
    }

    pub fn header() -> ansi_term::Style {
        ansi_term::Style::new().bold()
    }

    pub fn separator() -> ansi_term::Style {
        ansi_term::Style::new().dimmed()
    }
}

/// Format the success header for theme application
/// Output per 01-UI-SPEC.md Success Output Format:
/// "🎨 Theme Applied: {theme_name}" (header emoji + white bold theme name)
pub fn format_success_header(theme_name: &str) -> String {
    format!(
        "🎨 Theme Applied: {}",
        ColorScheme::header().paint(theme_name)
    )
}

/// Format a per-tool status line
/// For each tool line:
/// " {tool_name:<12} ━━━ {status_icon} {message}"
/// - tool_name left-aligned 12 chars
/// - Status icon: ✓ green for success, ✗ red for failure
/// - Message: "Updated" or error reason (max 60 chars per spec)
pub fn format_tool_status(tool_name: &str, is_success: bool, message: &str) -> String {
    let status_icon = if is_success {
        ColorScheme::success().paint("✓").to_string()
    } else {
        ColorScheme::failure().paint("✗").to_string()
    };

    let separator = ColorScheme::separator().paint("━━━").to_string();

    // Truncate message if too long
    let truncated_message = if message.len() > 60 {
        format!("{}...", &message[..57])
    } else {
        message.to_string()
    };

    format!(
        "    {:<12} {} {} {}",
        tool_name, separator, status_icon, truncated_message
    )
}

/// Format the summary line showing overall statistics
/// "N/M tools updated" or "N/M tools updated (X failed)"
/// Per for partial failure display
pub fn format_summary(successful: usize, total: usize, failed_count: usize) -> String {
    if failed_count == 0 {
        format!("{}/{} tools updated", successful, total)
    } else {
        format!(
            "{}/{} tools updated ({} failed)",
            successful, total, failed_count
        )
    }
}

/// Format an error message per CLUX-03 error message spec
/// "Error: {problem}"
/// Empty line
/// " Path: {path}"
/// " Problem: {detail}"
/// Empty line
/// "Guidance: {action}"
/// " $ {command_example}"
pub fn format_error(error: &ThemeError) -> String {
    match error {
        ThemeError::ThemeNotFound(name, available) => {
            format!(
                "Error: Theme not recognized\n\n    Problem: '{}' does not match any known theme\n\nGuidance: Use 'themectl set <name>' with one of:\n    Available: {}",
                name, available
            )
        }
        ThemeError::PartialFailure(_) => {
            format!(
                "Error: One or more tools failed to update\n\n    Problem: See per-tool status above\n\nGuidance: Check tool config syntax and permissions"
            )
        }
        ThemeError::NoToolsDetected => {
            format!(
                "Error: No supported tools detected\n\n    Problem: None of the supported tools were found on this system\n\nGuidance: Install at least one supported tool:\n    ghostty, starship, bat, delta, lazygit"
            )
        }
        _ => {
            format!("Error: {}", error)
        }
    }
}

/// Format verbose output for tool detection phase
/// "[Scanning for tools...]"
/// For each tool:
/// " Checking {tool}... Found at {path}" OR " Checking {tool}... Not found"
pub fn format_verbose_detection(tools: &[(String, Option<String>)]) -> String {
    let mut output = String::from("[Scanning for tools...]\n");

    for (tool, status_opt) in tools {
        if let Some(status) = status_opt {
            output.push_str(&format!("    Checking {}... {}\n", tool, status));
        } else {
            output.push_str(&format!("    Checking {}... Not found\n", tool));
        }
    }

    if output.ends_with('\n') {
        output.pop(); // Remove trailing newline
    }
    output
}

/// Format verbose output for theme application phase
/// " {tool}: {step} {detail}"
pub fn format_verbose_apply(tool: &str, step: &str, detail: &str) -> String {
    format!("    {}: {} {}", tool, step, detail)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_success_header_format() {
        let header = format_success_header("catppuccin-mocha");
        assert!(header.contains("🎨"));
        assert!(header.contains("Theme Applied:"));
        assert!(header.contains("catppuccin-mocha"));
    }

    #[test]
    fn test_success_status_line() {
        let line = format_tool_status("Ghostty", true, "Updated");
        assert!(line.contains("Ghostty"));
        assert!(line.contains("✓"));
        assert!(line.contains("Updated"));
        assert!(line.contains("━━━"));
    }

    #[test]
    fn test_failure_status_line() {
        let line = format_tool_status("Starship", false, "config parse error");
        assert!(line.contains("Starship"));
        assert!(line.contains("✗"));
        assert!(line.contains("config parse error"));
    }

    #[test]
    fn test_summary_all_success() {
        let summary = format_summary(3, 3, 0);
        assert_eq!(summary, "3/3 tools updated");
    }

    #[test]
    fn test_summary_with_failures() {
        let summary = format_summary(2, 3, 1);
        assert_eq!(summary, "2/3 tools updated (1 failed)");
    }

    #[test]
    fn test_tool_status_truncation() {
        let long_message = "a".repeat(70);
        let line = format_tool_status("Tool", true, &long_message);
        assert!(line.len() < long_message.len() + 100);
    }

    #[test]
    fn test_verbose_detection_found() {
        let tools = vec![
            (
                "Ghostty".to_string(),
                Some("Found at ~/.config/ghostty/config".to_string()),
            ),
            (
                "Starship".to_string(),
                Some("Installed (config will be created at ~/.config/starship.toml)".to_string()),
            ),
        ];
        let output = format_verbose_detection(&tools);
        assert!(output.contains("[Scanning for tools...]"));
        assert!(output.contains("Ghostty"));
        assert!(output.contains("Found at"));
        assert!(output.contains("config will be created at"));
    }

    #[test]
    fn test_verbose_detection_not_found() {
        let tools = vec![("bat".to_string(), None)];
        let output = format_verbose_detection(&tools);
        assert!(output.contains("bat"));
        assert!(output.contains("Not found"));
    }

    #[test]
    fn test_verbose_apply_format() {
        let line = format_verbose_apply("Ghostty", "Setting theme", "catppuccin-mocha");
        assert!(line.contains("Ghostty:"));
        assert!(line.contains("Setting theme"));
        assert!(line.contains("catppuccin-mocha"));
    }

    #[test]
    fn test_error_theme_not_found() {
        let error = ThemeError::ThemeNotFound(
            "invalid-theme".to_string(),
            "catppuccin-mocha, tokyo-night-dark".to_string(),
        );
        let formatted = format_error(&error);
        assert!(formatted.contains("Error:"));
        assert!(formatted.contains("invalid-theme"));
    }
}

/// Format the status header for status output
/// Simple header showing tool status information
pub fn format_status_header() -> String {
    format!(
        "{}{}",
        ColorScheme::header().paint("Tool Status"),
        ColorScheme::separator().paint(" ━━━━━━━━━━━━━━━━━━━━")
    )
}

/// Format a status line showing tool name and current theme
/// Format: " {tool_name:<12} {theme}"
/// - tool_name left-aligned in 12-char column
/// - theme on the right
pub fn format_status_line(tool: &str, theme: &str) -> String {
    format!("    {:<12} {}", tool, theme)
}

/// Format a list of restore points for plain-text output
/// Output format per line: "{id} {theme_name} {tools_joined_by_comma}"
/// No colors (suitable for piping/scripts)
pub fn format_restore_point_list(restore_points: &[crate::config::backup::RestorePoint]) -> String {
    if restore_points.is_empty() {
        return String::new();
    }

    let lines: Vec<String> = restore_points
        .iter()
        .map(|rp| {
            let tools_str = rp.tools.join(",");
            format!("{}  {}  {}", rp.id, rp.theme_name, tools_str)
        })
        .collect();

    lines.join("\n")
}

/// Format restore operation result
/// Shows which tools were restored successfully/failed
/// Matches output style: header + per-tool status + summary
pub fn format_restore_result(
    theme_name: &str,
    result: &crate::adapter::ApplyThemeResult,
) -> String {
    let mut output = String::new();

    // Header
    output.push_str(&format!(
        "{}{}",
        ColorScheme::header().paint("Restored: "),
        ColorScheme::header().paint(theme_name)
    ));
    output.push_str("\n\n");

    // Per-tool status for successful tools
    for tool in &result.successful {
        output.push_str(&format_tool_status(tool, true, "Restored"));
        output.push('\n');
    }

    // Per-tool status for failed tools
    for (tool, error) in &result.failed {
        output.push_str(&format_tool_status(tool, false, error));
        output.push('\n');
    }

    // Summary
    output.push('\n');
    let successful = result.successful.len();
    let total = successful + result.failed.len();
    let failed = result.failed.len();
    let summary = format_summary(successful, total, failed);
    output.push_str(&summary);

    output
}
