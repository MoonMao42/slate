//! Build-time labels, not provenance or a security checksum. Keep this std-only:
//! the native-helper regression fixture compiles build.rs directly with rustc.
use std::{
    env, fs,
    io::{self, Read},
    path::{Path, PathBuf},
};

// Keep in sync with non-Rust include_str!/include_bytes! inputs under src.
pub(crate) const FIXED_INPUTS: &[&str] = &[
    "Cargo.toml",
    "Cargo.lock",
    "build.rs",
    "build_metadata.rs",
    "themes/themes.toml",
    "resources/dark-mode-notify.swift",
    "resources/bat/slate.tmTheme.template",
    "resources/sfx/hero.wav",
    "resources/sfx/apply.wav",
    "resources/sfx/success.wav",
    "resources/sfx/failure.wav",
    "resources/sfx/select.wav",
    "resources/sfx/click.wav",
];
const MAX_ENTRIES: usize = 16_384;
const MAX_BYTES: u64 = 64 * 1024 * 1024;

pub(crate) fn emit() {
    // Watching the directory also catches new/deleted source files. Do not
    // watch .git, docs or target, or embed paths from the builder's machine.
    println!("cargo:rerun-if-changed=src");
    for path in FIXED_INPUTS {
        println!("cargo:rerun-if-changed={path}");
    }
    let root = env::var_os("CARGO_MANIFEST_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    let tag = source_tag(&root).unwrap_or_else(|_| "unavailable".to_owned());
    println!("cargo:rustc-env=SLATE_SOURCE_TAG={tag}");
    for (input, output) in [
        ("TARGET", "SLATE_BUILD_TARGET"),
        ("PROFILE", "SLATE_BUILD_PROFILE"),
    ] {
        let value = env::var(input).unwrap_or_default();
        let value = token(&value).unwrap_or("unknown");
        println!("cargo:rustc-env={output}={value}");
    }
    // Cargo supplies these for the selected build; inspect names, never values
    // of arbitrary environment variables. Sorting makes output deterministic.
    let mut features: Vec<_> = env::vars_os()
        .filter_map(|(key, _)| {
            let name = key.to_str()?.strip_prefix("CARGO_FEATURE_")?;
            Some(token(name)?.to_ascii_lowercase().replace('_', "-"))
        })
        .collect();
    features.sort();
    println!(
        "cargo:rustc-env=SLATE_BUILD_FEATURES={}",
        features.join(",")
    );
}

fn token(value: &str) -> Option<&str> {
    (!value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"-_.".contains(&c)))
    .then_some(value)
}

fn invalid() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, "source tag inputs unavailable")
}

/// FNV-1a64 over sorted, length-delimited relative paths and file bytes. No
/// mtimes/absolute paths/Git/toolchain output. Equal tags are not binary equality.
pub(crate) fn source_tag(root: &Path) -> io::Result<String> {
    let mut files: Vec<_> = FIXED_INPUTS.iter().map(PathBuf::from).collect();
    let mut remaining_entries = MAX_ENTRIES;
    collect_rust(
        root,
        Path::new("src"),
        0,
        &mut remaining_entries,
        &mut files,
    )?;
    files.sort();
    let mut hash = Fnv64::new();
    hash.add(b"slate-source-tag-v1\0");
    let mut remaining_bytes = MAX_BYTES;
    for relative in files {
        // Reject symlink components and non-regular leaves before opening. This
        // is a snapshot of build inputs, not protection from concurrent editors.
        let mut path = root.to_owned();
        for part in relative.components() {
            path.push(part);
            if fs::symlink_metadata(&path)?.is_symlink() {
                return Err(invalid());
            }
        }
        let metadata = fs::symlink_metadata(&path)?;
        if !metadata.is_file() || metadata.len() > remaining_bytes {
            return Err(invalid());
        }
        let name = relative
            .components()
            .map(|part| part.as_os_str().to_str().ok_or_else(invalid))
            .collect::<io::Result<Vec<_>>>()?
            .join("/");
        hash.add(&(name.len() as u64).to_le_bytes());
        hash.add(name.as_bytes());
        hash.add(&metadata.len().to_le_bytes());
        let mut file = fs::File::open(path)?.take(metadata.len() + 1);
        let mut buffer = [0u8; 16 * 1024];
        let mut read_bytes = 0;
        loop {
            let read = file.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            read_bytes += read as u64;
            hash.add(&buffer[..read]);
        }
        if read_bytes != metadata.len() {
            return Err(invalid());
        }
        remaining_bytes -= read_bytes;
    }
    Ok(format!("fnv1a64-v1-{:016x}", hash.0))
}

fn collect_rust(
    root: &Path,
    relative: &Path,
    depth: usize,
    remaining: &mut usize,
    files: &mut Vec<PathBuf>,
) -> io::Result<()> {
    if depth > 64 || !fs::symlink_metadata(root.join(relative))?.is_dir() {
        return Err(invalid());
    }
    for entry in fs::read_dir(root.join(relative))? {
        *remaining = remaining.checked_sub(1).ok_or_else(invalid)?;
        let entry = entry?;
        let path = relative.join(entry.file_name());
        let kind = entry.file_type()?;
        if kind.is_dir() {
            collect_rust(root, &path, depth + 1, remaining, files)?;
        } else if kind.is_symlink() {
            return Err(invalid());
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            if !kind.is_file() {
                return Err(invalid());
            }
            files.push(path);
        }
    }
    Ok(())
}

struct Fnv64(u64);

impl Fnv64 {
    fn new() -> Self {
        Self(0xcbf29ce484222325)
    }

    fn add(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.0 ^= u64::from(*byte);
            self.0 = self.0.wrapping_mul(0x100000001b3);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fnv_known_vector_and_metadata_tokens() {
        let mut hash = Fnv64::new();
        hash.add(b"hello");
        assert_eq!(hash.0, 0xa430d84680aabd0b);
        assert_eq!(token("aarch64-apple-darwin"), Some("aarch64-apple-darwin"));
        for invalid in [
            "",
            "debug\ncargo:rustc-env=INJECTED=1",
            "\x1b[31m",
            "../secret/",
        ] {
            assert!(token(invalid).is_none());
        }
    }
}
