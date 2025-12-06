/// Typography hierarchy helpers for setup wizard output.
/// Provides reusable text-emphasis primitives to improve visual hierarchy
/// without noisy decoration. All helpers are optional — Claude discretion on usage.
use crate::design::colors::Colors;
use crate::design::symbols::Symbols;

pub struct Typography;

impl Typography {
    /// Format a section header: "✦ " + gray text for structural clarity
    /// Example: "✦ Tool Inventory"
    pub fn section_header(title: &str) -> String {
        format!("{} {}", Symbols::BRAND, title)
    }

    /// Format a prominent label for key information
    /// Uses accent color to make it stand out
    /// Example: "❯ Your terminal is now beautiful!"
    pub fn strong_emphasis(text: &str) -> String {
        format!("{}{}", Colors::accent("❯ "), text)
    }

    /// Format a secondary label or contextual hint in dimmed gray
    /// Example: "current font: JetBrains Mono"
    pub fn secondary_label(label: &str, value: &str) -> String {
        format!("{}{}: {}", Colors::gray("  "), label, value)
    }

    /// Format a list item with visual consistency
    /// Uses symbol + label + description pattern
    /// Example: " ✓ ghostty — Makes your terminal glow"
    pub fn list_item(symbol: char, label: &str, description: &str) -> String {
        format!("  {} {} — {}", symbol, label, description)
    }

    /// Format a divider line in gray for visual separation
    /// Stays subtle to avoid clutter
    pub fn divider(width: usize) -> String {
        let line = "─".repeat(width);
        Colors::gray(&line)
    }

    /// Format success feedback with checkmark
    /// Example: "✓ Ghostty installed"
    pub fn success_message(message: &str) -> String {
        format!("{} {}", Symbols::SUCCESS, message)
    }

    /// Format warning/pending feedback with pending symbol
    /// Example: "○ fastfetch not installed"
    pub fn pending_message(message: &str) -> String {
        format!("{} {}", Symbols::PENDING, message)
    }

    /// Format failure feedback with X symbol
    /// Example: "✗ Network unreachable"
    pub fn failure_message(message: &str) -> String {
        format!("{} {}", Symbols::FAILURE, message)
    }

    /// Format a category heading with consistent styling
    /// Used in tool inventory, review receipt, etc.
    pub fn category_heading(name: &str) -> String {
        format!("{}→ {}", Colors::gray(""), name)
    }

    /// Format indented explanation text (for activation guidance, notes, etc.)
    /// Keeps it readable without overwhelming the output
    pub fn explanation(text: &str) -> String {
        format!("  {}", Colors::gray_dim(text))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_section_header_includes_brand_symbol() {
        let header = Typography::section_header("Tool Inventory");
        assert!(header.contains("✦"));
        assert!(header.contains("Tool Inventory"));
    }

    #[test]
    fn test_strong_emphasis_uses_accent() {
        let emphasized = Typography::strong_emphasis("Your terminal is now beautiful!");
        assert!(emphasized.contains("❯"));
        assert!(emphasized.contains("Your terminal is now beautiful!"));
    }

    #[test]
    fn test_secondary_label_format() {
        let label = Typography::secondary_label("font", "JetBrains Mono");
        assert!(label.contains("font"));
        assert!(label.contains("JetBrains Mono"));
    }

    #[test]
    fn test_list_item_format() {
        let item = Typography::list_item('✓', "ghostty", "Makes your terminal glow");
        assert!(item.contains("✓"));
        assert!(item.contains("ghostty"));
        assert!(item.contains("Makes your terminal glow"));
    }

    #[test]
    fn test_divider_creates_line() {
        let divider = Typography::divider(20);
        assert!(divider.contains("─"));
    }

    #[test]
    fn test_success_message_includes_checkmark() {
        let msg = Typography::success_message("Ghostty installed");
        assert!(msg.contains("✓"));
        assert!(msg.contains("Ghostty installed"));
    }

    #[test]
    fn test_pending_message_includes_symbol() {
        let msg = Typography::pending_message("fastfetch not installed");
        assert!(msg.contains("○"));
        assert!(msg.contains("fastfetch not installed"));
    }

    #[test]
    fn test_failure_message_includes_symbol() {
        let msg = Typography::failure_message("Network unreachable");
        assert!(msg.contains("✗"));
        assert!(msg.contains("Network unreachable"));
    }

    #[test]
    fn test_category_heading_format() {
        let heading = Typography::category_heading("Install");
        assert!(heading.contains("→"));
        assert!(heading.contains("Install"));
    }

    #[test]
    fn test_explanation_indented() {
        let explanation = Typography::explanation("This will be applied after restart");
        assert!(explanation.starts_with("  "));
        assert!(explanation.contains("This will be applied after restart"));
    }

    #[test]
    fn test_all_helpers_are_non_empty() {
        assert!(!Typography::section_header("Test").is_empty());
        assert!(!Typography::strong_emphasis("Test").is_empty());
        assert!(!Typography::secondary_label("label", "value").is_empty());
        assert!(!Typography::list_item('✓', "item", "desc").is_empty());
        assert!(!Typography::divider(10).is_empty());
    }
}

/// Format a line with 2 spaces left padding 
/// Used for all non-cliclack output (status, list, afterglow)
pub fn padded_line(content: &str) -> String {
    format!("  {}", content)
}

/// Format a section header for panels
/// Example: "✦ Core Vibe" for status dashboard sections 
pub fn section_header(symbol: &str, title: &str) -> String {
    padded_line(&format!("{} {}", symbol, title))
}

/// Format a label-value pair with proper color hierarchy
/// Labels in subtext (gray), values in text (bright)
/// Returns unformatted; caller applies ANSI codes per theme
pub fn label_value_pair(label: &str, value: &str) -> String {
    padded_line(&format!("{}    {}", label, value))
}
