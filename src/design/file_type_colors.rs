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
///
/// Precedence (locked per RESEARCH.md §file_type_colors):
/// 1. FileKind::Directory → FileDir
/// 2. FileKind::Symlink → FileSymlink
/// 3. FileKind::Executable → FileExec
/// 4. Name starts with '.' (and is not "." or "..") → FileHidden
/// 5. Name matches a full-filename entry (e.g. `Cargo.lock`) → matching variant
/// 6. Extension match → corresponding variant
/// 7. No match → FileDocs (foreground-tinted prose default)
pub fn classify(name: &str, kind: FileKind) -> SemanticColor {
    match kind {
        FileKind::Directory => return SemanticColor::FileDir,
        FileKind::Symlink => return SemanticColor::FileSymlink,
        FileKind::Executable => return SemanticColor::FileExec,
        FileKind::Regular => {}
    }

    // Hidden check: leading '.' but not "." or ".."
    if name.starts_with('.') && name != "." && name != ".." {
        return SemanticColor::FileHidden;
    }

    // Full-filename lock-file / manifest matches — must run before extension
    // lookup because `Cargo.lock` and `package-lock.json` are config meta-files.
    for (fname, role) in FULL_NAME_MAP {
        if *fname == name {
            return *role;
        }
    }

    // Extension lookup. Split on the LAST '.' — names without an extension
    // (e.g., `Makefile`, `deploy`) fall through to the default.
    let ext = match name.rsplit_once('.') {
        Some((_, ext)) if !ext.is_empty() => ext.to_ascii_lowercase(),
        _ => return SemanticColor::FileDocs,
    };

    for (known_ext, role) in EXTENSION_MAP {
        if *known_ext == ext {
            return *role;
        }
    }

    SemanticColor::FileDocs
}

/// Flat (extension-without-dot, SemanticColor) pairs for Phase 16
/// `LS_COLORS` / `EZA_COLORS` generation. Order is deterministic — Phase 16
/// iterates this slice to emit colors per extension.
pub fn extension_map() -> &'static [(&'static str, SemanticColor)] {
    EXTENSION_MAP
}

/// Full-filename matches (e.g. lock files, manifests) that should NOT fall
/// through to extension lookup.
static FULL_NAME_MAP: &[(&str, SemanticColor)] = &[
    ("Cargo.lock", SemanticColor::FileConfig),
    ("package-lock.json", SemanticColor::FileConfig),
    ("yarn.lock", SemanticColor::FileConfig),
    ("pnpm-lock.yaml", SemanticColor::FileConfig),
];

/// Extension-to-role mapping (no leading dot; lowercased at lookup time).
static EXTENSION_MAP: &[(&str, SemanticColor)] = &[
    // Archives / compressed
    ("tar", SemanticColor::FileArchive),
    ("tgz", SemanticColor::FileArchive),
    ("zip", SemanticColor::FileArchive),
    ("gz", SemanticColor::FileArchive),
    ("bz2", SemanticColor::FileArchive),
    ("xz", SemanticColor::FileArchive),
    ("7z", SemanticColor::FileArchive),
    ("rar", SemanticColor::FileArchive),
    ("zst", SemanticColor::FileArchive),
    // Images
    ("png", SemanticColor::FileImage),
    ("jpg", SemanticColor::FileImage),
    ("jpeg", SemanticColor::FileImage),
    ("gif", SemanticColor::FileImage),
    ("svg", SemanticColor::FileImage),
    ("webp", SemanticColor::FileImage),
    ("bmp", SemanticColor::FileImage),
    ("ico", SemanticColor::FileImage),
    // Video
    ("mp4", SemanticColor::FileMedia),
    ("mkv", SemanticColor::FileMedia),
    ("avi", SemanticColor::FileMedia),
    ("mov", SemanticColor::FileMedia),
    ("webm", SemanticColor::FileMedia),
    // Audio
    ("mp3", SemanticColor::FileAudio),
    ("flac", SemanticColor::FileAudio),
    ("wav", SemanticColor::FileAudio),
    ("ogg", SemanticColor::FileAudio),
    ("m4a", SemanticColor::FileAudio),
    // Source code
    ("ts", SemanticColor::FileCode),
    ("rs", SemanticColor::FileCode),
    ("py", SemanticColor::FileCode),
    ("js", SemanticColor::FileCode),
    ("go", SemanticColor::FileCode),
    ("c", SemanticColor::FileCode),
    ("cpp", SemanticColor::FileCode),
    ("rb", SemanticColor::FileCode),
    ("java", SemanticColor::FileCode),
    ("swift", SemanticColor::FileCode),
    // Docs
    ("md", SemanticColor::FileDocs),
    ("txt", SemanticColor::FileDocs),
    ("rst", SemanticColor::FileDocs),
    ("org", SemanticColor::FileDocs),
    ("adoc", SemanticColor::FileDocs),
    // Config
    ("toml", SemanticColor::FileConfig),
    ("yaml", SemanticColor::FileConfig),
    ("yml", SemanticColor::FileConfig),
    ("json", SemanticColor::FileConfig),
    ("ini", SemanticColor::FileConfig),
];

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
