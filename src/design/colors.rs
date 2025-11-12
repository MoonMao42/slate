/// Design system color palette.
/// Strategy
/// - Structural elements (lines, separators, labels) → GRAY (neutral)
/// - Brand highlights (logo, key moments) → ACCENT (fixed brand color, not from any theme)
/// - Theme-specific text → theme colors (populated at runtime)
pub struct Colors;

impl Colors {
    /// Neutral gray for structural elements (separators, labels, borders)
    /// Using ANSI 256-color palette (244 = medium gray, not too dark for legibility)
    pub const GRAY: &str = "\x1b[38;5;244m";

    /// Fixed brand accent color — chosen to NOT appear in any theme palette
    /// Using bright purple (not in Catppuccin, Tokyo Night, Dracula, or Nord)
    /// RGB: #8B5CF6 (Tailwind purple-500 family)
    pub const ACCENT: &str = "\x1b[38;2;139;92;246m";

    /// Reset to default terminal color
    pub const RESET: &str = "\x1b[0m";

    /// Muted gray for secondary text
    pub const GRAY_DIM: &str = "\x1b[38;5;240m";

    /// Helper: format a string with gray color
    pub fn gray(text: &str) -> String {
        format!("{}{}{}", Self::GRAY, text, Self::RESET)
    }

    /// Helper: format a string with dimmed gray color
    pub fn gray_dim(text: &str) -> String {
        format!("{}{}{}", Self::GRAY_DIM, text, Self::RESET)
    }

    /// Helper: format a string with accent color
    pub fn accent(text: &str) -> String {
        format!("{}{}{}", Self::ACCENT, text, Self::RESET)
    }

    /// Helper: format label and separator in gray
    /// Example: "current:" where both "current" and ":" are gray
    pub fn label(name: &str, separator: &str) -> String {
        format!("{}{}{}{}", Self::GRAY, name, separator, Self::RESET)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_colors_format() {
        let gray_text = Colors::gray("test");
        assert!(gray_text.contains("\x1b[38;5;244m"));
        assert!(gray_text.contains("\x1b[0m"));
    }

    #[test]
    fn test_gray_dim_format() {
        let gray_dim_text = Colors::gray_dim("test");
        assert!(gray_dim_text.contains("\x1b[38;5;240m"));
        assert!(gray_dim_text.contains("\x1b[0m"));
    }

    #[test]
    fn test_accent_format() {
        let accent_text = Colors::accent("✦");
        assert!(accent_text.contains("\x1b[38;2;139;92;246m"));
    }

    #[test]
    fn test_label_format() {
        let label_text = Colors::label("current", ":");
        assert!(label_text.contains("current"));
        assert!(label_text.contains(":"));
        assert!(label_text.contains("\x1b["));
        assert_eq!(label_text.matches(Colors::RESET).count(), 1);
    }

    #[test]
    fn test_reset_code() {
        assert_eq!(Colors::RESET, "\x1b[0m");
    }
}
