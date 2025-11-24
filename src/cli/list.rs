use crate::error::Result;
use crate::brand::language::Language;
use crate::theme::ThemeRegistry;
use crate::adapter::palette_renderer::PaletteRenderer;

/// Handle `slate list` command
/// Show themes with TrueColor palette preview blocks
pub fn handle(_args: &[&str]) -> Result<()> {
    let registry = ThemeRegistry::new()?;
    
    println!("{}", Language::LIST_HEADER);
    println!();
    
    for variant in registry.all() {
        // Theme name in bold
        print!("  {} ", variant.name);
        
        // TrueColor palette blocks (████████ per color)
        // Show 4 representative colors: foreground, background, accent, error
        let colors = vec![
            &variant.palette.foreground,
            &variant.palette.background,
            &variant.palette.blue,
            &variant.palette.red,
        ];
        
        for hex_color in colors {
            if let Ok((r, g, b)) = PaletteRenderer::hex_to_rgb(hex_color) {
                // ANSI 24-bit TrueColor escape sequence: ESC[38;2;R;G;Bm
                let block = format!("[38;2;{};{};{}m████[0m ", r, g, b);
                print!("{}", block);
            }
        }
        
        // Optional: Family/description
        println!("  {} ", variant.family);
    }
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_handle_no_args() {
        let result = handle(&[]);
        assert!(result.is_ok());
    }
}
