use crate::error::Result;
use crate::design::symbols::Symbols;

/// Handle `slate set <theme>` command
pub fn handle(args: &[&str]) -> Result<()> {
    // will implement full theme switching

    if let Some(theme) = args.first() {
        println!("{} {} — implemented in ", Symbols::SUCCESS, theme);
    } else {
        println!("Interactive theme picker — implemented in ");
    }

    Ok(())
}
