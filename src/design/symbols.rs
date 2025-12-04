/// Design system symbol language.
/// Used consistently across all user-facing output.
pub struct Symbols;

impl Symbols {
    /// Brand logo marker
    pub const BRAND: char = '✦';

    /// Task completed successfully
    pub const SUCCESS: char = '✓';

    /// Task failed or error state
    pub const FAILURE: char = '✗';

    /// Pending state or not installed
    pub const PENDING: char = '○';

    /// Call-to-action arrow (used in receipts, action lists)
    pub const CTA_ARROW: char = '→';

    /// Chevron/continuation marker (used in hierarchical displays)
    pub const CHEVRON: char = '❯';

    /// Diamond marker (used for special/metadata info)
    pub const DIAMOND: char = '◆';

    /// Preferences/settings icon
    pub const PREFERENCES: char = '⚙';

    /// Quit/exit icon
    pub const QUIT: char = '⏊';

    /// Back/return icon
    pub const BACK: char = '←';
}

// Example usage:
// println!("{} Theme applied", Symbols::SUCCESS);
// println!("{} Ghostty not installed", Symbols::PENDING);
// println!("{} Install", Symbols::CTA_ARROW);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_symbols_are_chars() {
        assert_eq!(Symbols::BRAND, '✦');
        assert_eq!(Symbols::SUCCESS, '✓');
        assert_eq!(Symbols::FAILURE, '✗');
        assert_eq!(Symbols::PENDING, '○');
        assert_eq!(Symbols::CTA_ARROW, '→');
        assert_eq!(Symbols::CHEVRON, '❯');
        assert_eq!(Symbols::DIAMOND, '◆');
        assert_eq!(Symbols::PREFERENCES, '⚙');
        assert_eq!(Symbols::QUIT, '⏊');
        assert_eq!(Symbols::BACK, '←');
    }

    #[test]
    fn test_symbols_printable() {
        // Ensure symbols can be used in format strings
        let formatted = format!("{} Success message", Symbols::SUCCESS);
        assert!(formatted.contains("✓"));

        let cta = format!("{} Install tools", Symbols::CTA_ARROW);
        assert!(cta.contains("→"));
    }

    #[test]
    fn test_symbol_distinctiveness() {
        // Verify each symbol is different from others
        let symbols = vec![
            Symbols::BRAND,
            Symbols::SUCCESS,
            Symbols::FAILURE,
            Symbols::PENDING,
            Symbols::CTA_ARROW,
            Symbols::CHEVRON,
            Symbols::DIAMOND,
            Symbols::PREFERENCES,
            Symbols::QUIT,
            Symbols::BACK,
        ];

        for (i, &sym1) in symbols.iter().enumerate() {
            for &sym2 in symbols.iter().skip(i + 1) {
                assert_ne!(sym1, sym2, "Symbols must be distinct for clarity");
            }
        }
    }
}
