/// Font selection for setup wizard.
/// through
/// Font option for wizard selection
#[derive(Debug, Clone)]
pub struct FontOption {
    /// Font identifier (e.g., "jetbrains-mono")
    pub id: &'static str,
    /// Display name (e.g., "JetBrains Mono Nerd Font")
    pub name: &'static str,
    /// Brief recommendation label
    pub label: &'static str,
    /// Homebrew cask package name (e.g., "font-jetbrains-mono-nerd-font")
    pub brew_cask: &'static str,
    /// Official Nerd Fonts release asset stem (e.g., "JetBrainsMono")
    pub release_asset: &'static str,
}

/// Font catalog: all available Nerd Font options for setup
pub struct FontCatalog;

impl FontCatalog {
    /// Get all available font options
    pub fn all_fonts() -> Vec<FontOption> {
        vec![
            FontOption {
                id: "jetbrains-mono",
                name: "JetBrains Mono Nerd Font",
                label: "terminal favorite",
                brew_cask: "font-jetbrains-mono-nerd-font",
                release_asset: "JetBrainsMono",
            },
            FontOption {
                id: "fira-code",
                name: "Fira Code Nerd Font",
                label: "ligature lover",
                brew_cask: "font-fira-code-nerd-font",
                release_asset: "FiraCode",
            },
            FontOption {
                id: "iosevka-term",
                name: "Iosevka Term Nerd Font",
                label: "compact & dense",
                brew_cask: "font-iosevka-term-nerd-font",
                release_asset: "IosevkaTerm",
            },
            FontOption {
                id: "hack",
                name: "Hack Nerd Font",
                label: "clean classic",
                brew_cask: "font-hack-nerd-font",
                release_asset: "Hack",
            },
        ]
    }

    /// Get font option by ID
    pub fn get_font(id: &str) -> Option<FontOption> {
        Self::all_fonts().into_iter().find(|f| f.id == id)
    }

    /// Get recommended default font
    pub fn default_font() -> FontOption {
        Self::get_font("jetbrains-mono").expect("Default font must exist")
    }

    /// Get the "skip" option (keep current font)
    pub fn skip_option() -> (&'static str, &'static str) {
        ("skip", "Skip (keep current font)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_fonts_exist() {
        let fonts = FontCatalog::all_fonts();
        assert_eq!(fonts.len(), 4, "Must have exactly 4 font options per ");
    }

    #[test]
    fn test_font_ids_unique() {
        let fonts = FontCatalog::all_fonts();
        let mut ids = vec![];
        for f in &fonts {
            assert!(!ids.contains(&f.id), "Font ID must be unique: {}", f.id);
            ids.push(f.id);
        }
    }

    #[test]
    fn test_font_brew_cask_names() {
        // Verify all brew cask names follow the pattern
        for font in FontCatalog::all_fonts() {
            assert!(
                font.brew_cask.starts_with("font-") && font.brew_cask.ends_with("-nerd-font"),
                "Invalid brew cask name: {}",
                font.brew_cask
            );
        }
    }

    #[test]
    fn test_font_release_assets_exist() {
        for font in FontCatalog::all_fonts() {
            assert!(!font.release_asset.is_empty());
            assert!(!font.release_asset.contains(' '));
        }
    }

    #[test]
    fn test_default_font_is_jetbrains() {
        let default = FontCatalog::default_font();
        assert_eq!(default.id, "jetbrains-mono");
    }

    #[test]
    fn test_skip_option_available() {
        let (skip_id, skip_label) = FontCatalog::skip_option();
        assert_eq!(skip_id, "skip");
        assert!(!skip_label.is_empty());
    }
}
