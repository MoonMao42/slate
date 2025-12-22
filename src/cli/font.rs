use crate::adapter::font::FontAdapter;
use crate::design::symbols::Symbols;
use crate::env::SlateEnv;
use crate::error::Result;

/// Handle `slate font` command
/// Supports two modes:
/// 1. `slate font <name>` — Apply explicit font directly
/// 2. `slate font` (no args) — Launch interactive font picker
pub fn handle_font(font_name: Option<String>) -> Result<()> {
    if let Some(name) = font_name {
        // Direct apply path: apply font name to terminal adapters
        let env = SlateEnv::from_process()?;
        
        FontAdapter::apply_font(&env, &name)?;
        
        // Trigger Ghostty reload via adapter 
        let adapter_registry = crate::adapter::ToolRegistry::default();
        if let Some(ghostty_adapter) = adapter_registry.get_adapter("ghostty") {
            let _ = ghostty_adapter.reload();
        }
        
        println!("{} Font switched to '{}'", Symbols::SUCCESS, name);
        Ok(())
    } else {
        // Picker path: show font picker UI (to be implemented per)
        // For now, delegate to an empty picker that would be filled in by 07-02
        // This is sufficient for 07-01 acceptance criteria which expects the hook point to exist
        println!("{} Font picker not yet implemented (scheduled for 07-02)", Symbols::PENDING);
        Ok(())
    }
}
