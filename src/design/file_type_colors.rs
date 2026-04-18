//! File-type → SemanticColor classification.
//!
//! Phase 15 (`slate demo`) consumes this to color the directory-tree block.
//! Phase 16 (`LS_COLORS` / `EZA_COLORS` generation) will consume the same
//! classifier so colors stay identical between the demo tree and real `ls`.

use crate::cli::picker::preview_panel::SemanticColor;

/// Filesystem-type hint supplied by the caller. Extension classification alone
/// cannot determine directory/symlink/executable status — the caller provides it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileKind {
    Regular,
    Directory,
    Symlink,
    Executable,
}

/// Classify an entry by name + filesystem kind into its SemanticColor role.
/// STUB — Plan 02 (Wave 1b) replaces this body with the real mapping.
pub fn classify(_name: &str, _kind: FileKind) -> SemanticColor {
    SemanticColor::FileDocs
}

/// Flat (extension, SemanticColor) pairs for Phase 16 LS_COLORS/EZA_COLORS generation.
/// STUB — Plan 02 (Wave 1b) replaces this body with the real mapping.
pub fn extension_map() -> &'static [(&'static str, SemanticColor)] {
    &[]
}
