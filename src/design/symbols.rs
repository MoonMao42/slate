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
    }

    #[test]
    fn test_symbols_printable() {
        // Ensure symbols can be used in format strings
        let formatted = format!("{} Success message", Symbols::SUCCESS);
        assert!(formatted.contains("✓"));
    }
}
