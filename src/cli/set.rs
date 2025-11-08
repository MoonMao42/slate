use crate::error::Result;
use crate::brand::language::Language;
use crate::design::symbols::Symbols;

/// Handle `slate set <theme>` command
pub fn handle(args: &[&str]) -> Result<()> {
    // will implement full theme switching

    if let Some(theme) = args.first() {
        println!("{} {}", Symbols::SUCCESS, Language::set_pending_theme(theme));
    } else {
        println!("{}", Language::SET_PICKER_PENDING);
    }

    Ok(())
}
