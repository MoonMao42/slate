//! Preview panel showing sample shell output with theme colors and ANSI matrix.
//! Provides SemanticColor enum mapping and hardcoded
//! sample token flow for picker inline preview. No rendering logic here; just data structures.

/// Semantic color roles for consistent rendering across adapters.
/// Each role maps to a palette color via Palette::resolve().
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SemanticColor {
    // Git-related
    GitBranch,    // Main branch names (git status prompt suffix)
    GitAdded,     // Green (git status A files)
    GitModified,  // Yellow (git status M files)
    GitUntracked, // Red (git status ?? files)

    // File system
    Directory,   // Directory paths
    FileExec,    // Executable files
    FileSymlink, // Symbolic links
    FileDir,     // Directory in listing

    // Prompt & interaction
    Prompt, // $ / % prompt character
    Accent, // Highlight color (e.g., starship module)
    Error,  // Error messages
    Muted,  // Dimmed text (comments, helpers)

    // Starship/shell specific
    Success, // Command exit success (green star, etc.)
    Warning, // Command exit warning
    Failed,  // Command exit failure (red cross, etc.)
    Status,  // Status indicators

    // Text levels
    Text,    // Default text color
    Subtext, // Secondary text (metadata)

    // Syntax highlighting (consumed by `slate demo` and future editor adapter)
    Keyword,
    String,
    Comment,
    Function,
    Number,
    Type,

    // File-type classification (shared with LS_COLORS/EZA_COLORS)
    FileArchive,
    FileImage,
    FileMedia,
    FileAudio,
    FileCode,
    FileDocs,
    FileConfig,
    FileHidden,
}

/// A single span in the preview sample output with associated semantic color role.
#[derive(Debug, Clone)]
pub struct PreviewSpan {
    pub text: &'static str,
    pub role: SemanticColor,
}

/// Hardcoded sample tokens showing realistic shell output.
/// Uses real project files (Cargo.toml, src/, README.md) and git status format.
pub const SAMPLE_TOKENS: &[PreviewSpan] = &[
    // Prompt + directory + branch
    PreviewSpan {
        text: "~/code/slate",
        role: SemanticColor::Directory,
    },
    PreviewSpan {
        text: " ",
        role: SemanticColor::Muted,
    },
    PreviewSpan {
        text: "main",
        role: SemanticColor::GitBranch,
    },
    PreviewSpan {
        text: "*",
        role: SemanticColor::GitModified,
    },
    PreviewSpan {
        text: "\n",
        role: SemanticColor::Muted,
    },
    // First command: ls
    PreviewSpan {
        text: "$ ",
        role: SemanticColor::Prompt,
    },
    PreviewSpan {
        text: "ls",
        role: SemanticColor::Muted,
    },
    PreviewSpan {
        text: "\n",
        role: SemanticColor::Muted,
    },
    // File listing
    PreviewSpan {
        text: "src",
        role: SemanticColor::Directory,
    },
    PreviewSpan {
        text: "   ",
        role: SemanticColor::Muted,
    },
    PreviewSpan {
        text: "tests",
        role: SemanticColor::Directory,
    },
    PreviewSpan {
        text: "   ",
        role: SemanticColor::Muted,
    },
    PreviewSpan {
        text: "Cargo.toml",
        role: SemanticColor::Muted,
    },
    PreviewSpan {
        text: "\n",
        role: SemanticColor::Muted,
    },
    // Second command: git status
    PreviewSpan {
        text: "$ ",
        role: SemanticColor::Prompt,
    },
    PreviewSpan {
        text: "git status",
        role: SemanticColor::Muted,
    },
    PreviewSpan {
        text: "\n",
        role: SemanticColor::Muted,
    },
    // Git status output
    PreviewSpan {
        text: "M  src/cli/picker.rs",
        role: SemanticColor::GitModified,
    },
    PreviewSpan {
        text: "\n",
        role: SemanticColor::Muted,
    },
    PreviewSpan {
        text: "?? new.rs",
        role: SemanticColor::GitUntracked,
    },
];

/// Render preview panel showing sample tokens and ANSI color matrices.
/// Output sample token lines, 16 ANSI matrix, and optional extras matrix.
/// Returns formatted string with ANSI 24-bit escape codes embedded so the output
/// renders in color when written to a real terminal.
pub fn render_preview(palette: &crate::theme::Palette) -> String {
    use crate::adapter::palette_renderer::PaletteRenderer;

    const RESET: &str = "\x1b[0m";
    let bg = |hex: &str| -> String {
        let (r, g, b) = PaletteRenderer::hex_to_rgb(hex).unwrap_or((200, 200, 200));
        format!("\x1b[48;2;{};{};{}m", r, g, b)
    };

    let mut output = String::new();

    // Render 16 ANSI color matrix using background blocks so every cell
    // carries an explicit \x1b[48;2;R;G;Bm sequence.
    // Normal (0-7)
    output.push_str("Normal: ");
    let ansi_normal = [
        &palette.black,
        &palette.red,
        &palette.green,
        &palette.yellow,
        &palette.blue,
        &palette.magenta,
        &palette.cyan,
        &palette.white,
    ];
    for (idx, color) in ansi_normal.iter().enumerate() {
        output.push_str(&bg(color));
        output.push_str(&format!(" {} ", idx));
        output.push_str(RESET);
        output.push(' ');
    }
    output.push('\n');

    // Bright (8-15)
    output.push_str("Bright: ");
    let ansi_bright = [
        &palette.bright_black,
        &palette.bright_red,
        &palette.bright_green,
        &palette.bright_yellow,
        &palette.bright_blue,
        &palette.bright_magenta,
        &palette.bright_cyan,
        &palette.bright_white,
    ];
    for (idx, color) in ansi_bright.iter().enumerate() {
        output.push_str(&bg(color));
        output.push_str(&format!(" {} ", idx + 8));
        output.push_str(RESET);
        output.push(' ');
    }
    output.push('\n');

    // Render extras matrix if presentconditional)
    if !palette.extras.is_empty() {
        output.push_str("Extras: ");
        let mut sorted_extras: Vec<_> = palette.extras.iter().collect();
        sorted_extras.sort_by_key(|(name, _)| *name);
        let mut extra_count = 0;
        for (name, color) in &sorted_extras {
            output.push_str(&bg(color));
            output.push_str(&format!(" {} ", name));
            output.push_str(RESET);
            output.push(' ');
            extra_count += 1;
            if extra_count >= 8 && extra_count % 8 == 0 {
                output.push_str("\n        ");
            }
        }
        output.push('\n');
    }

    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_semantic_color_enum_exists() {
        let _: SemanticColor = SemanticColor::GitBranch;
        let _: SemanticColor = SemanticColor::Directory;
        let _: SemanticColor = SemanticColor::Error;
    }

    #[test]
    fn test_preview_span_struct_exists() {
        let span = PreviewSpan {
            text: "test",
            role: SemanticColor::Muted,
        };
        assert_eq!(span.text, "test");
    }

    #[test]
    fn test_sample_tokens_all_roles_present() {
        let mut has_directory = false;
        let mut has_git_branch = false;
        let mut has_git_modified = false;
        let mut has_git_untracked = false;
        let mut has_prompt = false;
        let mut has_muted = false;

        for span in SAMPLE_TOKENS {
            match span.role {
                SemanticColor::Directory => has_directory = true,
                SemanticColor::GitBranch => has_git_branch = true,
                SemanticColor::GitModified => has_git_modified = true,
                SemanticColor::GitUntracked => has_git_untracked = true,
                SemanticColor::Prompt => has_prompt = true,
                SemanticColor::Muted => has_muted = true,
                _ => {}
            }
        }

        assert!(has_directory, "Sample tokens should include Directory role");
        assert!(
            has_git_branch,
            "Sample tokens should include GitBranch role"
        );
        assert!(
            has_git_modified,
            "Sample tokens should include GitModified role"
        );
        assert!(
            has_git_untracked,
            "Sample tokens should include GitUntracked role"
        );
        assert!(has_prompt, "Sample tokens should include Prompt role");
        assert!(has_muted, "Sample tokens should include Muted role");
    }
}
