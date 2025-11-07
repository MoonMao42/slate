use crate::error::Result;
use crate::brand::language::Language;
use crate::design::symbols::Symbols;

/// Handle `slate setup` command
pub fn handle(args: &[&str]) -> Result<()> {
    // will implement full wizard
    // For now: just show placeholder message

    if args.contains(&"--quick") {
        println!("{} {}", Symbols::BRAND, Language::SETUP_WELCOME);
        println!("Quick setup mode — implemented in ");
    } else {
        println!("{} {}", Symbols::BRAND, Language::SETUP_WELCOME);
        println!("Interactive setup wizard — implemented in ");
    }

    Ok(())
}
