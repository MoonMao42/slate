//! Preview panel showing sample shell output with theme colors and ANSI matrix.
//! Per , Provides SemanticColor enum mapping and hardcoded
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
    Directory,  // Directory paths
    FileExec,   // Executable files
    FileSymlink, // Symbolic links
    FileDir,    // Directory in listing

    // Prompt & interaction
    Prompt,  // $ / % prompt character
    Accent,  // Highlight color (e.g., starship module)
    Error,   // Error messages
    Muted,   // Dimmed text (comments, helpers)

    // Starship/shell specific
    Success, // Command exit success (green star, etc.)
    Warning, // Command exit warning
    Failed,  // Command exit failure (red cross, etc.)
    Status,  // Status indicators

    // Text levels
    Text,    // Default text color
    Subtext, // Secondary text (metadata)
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
    fn test_sample_tokens_not_empty() {
        assert!(!SAMPLE_TOKENS.is_empty());
        assert!(SAMPLE_TOKENS.len() > 10);
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
        assert!(has_git_branch, "Sample tokens should include GitBranch role");
        assert!(has_git_modified, "Sample tokens should include GitModified role");
        assert!(has_git_untracked, "Sample tokens should include GitUntracked role");
        assert!(has_prompt, "Sample tokens should include Prompt role");
        assert!(has_muted, "Sample tokens should include Muted role");
    }
}
