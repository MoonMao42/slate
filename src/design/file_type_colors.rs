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

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case("src", FileKind::Directory, SemanticColor::FileDir)]
    #[case("deploy", FileKind::Executable, SemanticColor::FileExec)]
    #[case("link", FileKind::Symlink, SemanticColor::FileSymlink)]
    #[case("hero.png", FileKind::Regular, SemanticColor::FileImage)]
    #[case("image.JPG", FileKind::Regular, SemanticColor::FileImage)] // case-insensitive ext
    #[case("fonts.zip", FileKind::Regular, SemanticColor::FileArchive)]
    #[case("backup.TAR.GZ", FileKind::Regular, SemanticColor::FileArchive)] // last-dot split
    #[case("song.mp3", FileKind::Regular, SemanticColor::FileAudio)]
    #[case("clip.mp4", FileKind::Regular, SemanticColor::FileMedia)]
    #[case("index.ts", FileKind::Regular, SemanticColor::FileCode)]
    #[case("main.rs", FileKind::Regular, SemanticColor::FileCode)]
    #[case("README.md", FileKind::Regular, SemanticColor::FileDocs)]
    #[case("notes.txt", FileKind::Regular, SemanticColor::FileDocs)]
    #[case("package.json", FileKind::Regular, SemanticColor::FileConfig)]
    #[case("Cargo.lock", FileKind::Regular, SemanticColor::FileConfig)]
    #[case("pnpm-lock.yaml", FileKind::Regular, SemanticColor::FileConfig)]
    #[case(".gitignore", FileKind::Regular, SemanticColor::FileHidden)]
    #[case(".env", FileKind::Regular, SemanticColor::FileHidden)]
    #[case(".DS_Store", FileKind::Regular, SemanticColor::FileHidden)]
    #[case(".", FileKind::Regular, SemanticColor::FileDocs)] // lone dot
    #[case("..", FileKind::Regular, SemanticColor::FileDocs)] // lone double-dot
    #[case("mystery.xyz", FileKind::Regular, SemanticColor::FileDocs)] // unknown ext
    #[case("Makefile", FileKind::Regular, SemanticColor::FileDocs)] // no extension
    #[case(".hidden.ts", FileKind::Directory, SemanticColor::FileDir)] // directory wins
    #[case(".hidden.ts", FileKind::Executable, SemanticColor::FileExec)] // exec wins
    fn classify_matches_expected_role(
        #[case] name: &str,
        #[case] kind: FileKind,
        #[case] expected: SemanticColor,
    ) {
        assert_eq!(classify(name, kind), expected, "name={name} kind={kind:?}");
    }

    #[test]
    fn extension_map_is_non_empty_and_contains_expected_entries() {
        let map = extension_map();
        assert!(
            !map.is_empty(),
            "extension_map must be populated for Phase 16"
        );
        assert!(map.contains(&("ts", SemanticColor::FileCode)));
        assert!(map.contains(&("zip", SemanticColor::FileArchive)));
        assert!(map.contains(&("mp3", SemanticColor::FileAudio)));
        assert!(map.contains(&("png", SemanticColor::FileImage)));
        assert!(map.contains(&("toml", SemanticColor::FileConfig)));
    }

    #[test]
    fn extension_map_has_no_duplicate_keys() {
        let map = extension_map();
        let mut keys: Vec<&str> = map.iter().map(|(k, _)| *k).collect();
        keys.sort_unstable();
        let original_len = keys.len();
        keys.dedup();
        assert_eq!(keys.len(), original_len, "extension_map has duplicate keys");
    }
}
