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

    /// Diamond marker (used for special/metadata info)
    pub const DIAMOND: char = '◆';
}

// Example usage:
// println!("{} Theme applied", Symbols::SUCCESS);
// println!("{} Ghostty not installed", Symbols::PENDING);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_symbols_are_chars() {
        assert_eq!(Symbols::BRAND, '✦');
        assert_eq!(Symbols::SUCCESS, '✓');
        assert_eq!(Symbols::FAILURE, '✗');
        assert_eq!(Symbols::PENDING, '○');
        assert_eq!(Symbols::DIAMOND, '◆');
    }

    #[test]
    fn test_symbols_printable() {
        // Ensure symbols can be used in format strings
        let formatted = format!("{} Success message", Symbols::SUCCESS);
        assert!(formatted.contains("✓"));

        let diamond = format!("{} Special info", Symbols::DIAMOND);
        assert!(diamond.contains("◆"));
    }

    #[test]
    fn test_symbol_distinctiveness() {
        // Verify each symbol is different from others
        let symbols = [
            Symbols::BRAND,
            Symbols::SUCCESS,
            Symbols::FAILURE,
            Symbols::PENDING,
            Symbols::DIAMOND,
        ];

        for (i, &sym1) in symbols.iter().enumerate() {
            for &sym2 in symbols.iter().skip(i + 1) {
                assert_ne!(sym1, sym2, "Symbols must be distinct for clarity");
            }
        }
    }
}
