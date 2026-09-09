use crate::env::SlateEnv;
use crate::platform::capabilities::CapabilityReport;
use std::path::{Path, PathBuf};

mod cache;
mod paths;
pub use cache::{activation_hint, activation_hint_in, refresh_font_cache, FontCacheRefresh};
pub(crate) use paths::install_directory;

/// OpenType filename extensions are case-insensitive, including collections.
pub(crate) fn supported_font_extension(path: &Path) -> bool {
    path.extension().is_some_and(|extension| {
        ["ttf", "otf", "ttc", "otc"]
            .iter()
            .any(|suffix| extension.eq_ignore_ascii_case(suffix))
    })
}

/// Signature only, not SFNT tables, names, glyphs or native activation.
pub(crate) fn has_sfnt_signature(prefix: &[u8]) -> bool {
    prefix.len() >= 12
        && matches!(
            prefix.get(..4),
            Some(b"\0\x01\0\0" | b"OTTO" | b"ttcf" | b"true" | b"typ1")
        )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FontPlatformBackend {
    Macos,
    Fontconfig,
}

impl FontPlatformBackend {
    pub fn label(self) -> &'static str {
        match self {
            Self::Macos => "macOS fonts",
            Self::Fontconfig => "fontconfig",
        }
    }
}

pub fn backend() -> FontPlatformBackend {
    if cfg!(target_os = "macos") {
        FontPlatformBackend::Macos
    } else {
        FontPlatformBackend::Fontconfig
    }
}

pub fn capability_report() -> CapabilityReport {
    match backend() {
        FontPlatformBackend::Macos => CapabilityReport::supported("macos-fonts"),
        FontPlatformBackend::Fontconfig => {
            if crate::detection::command_path("fc-cache").is_some() {
                CapabilityReport::supported("fontconfig")
            } else {
                CapabilityReport::missing_dependency(
                    "fontconfig",
                    "Install fontconfig (`fc-cache`) so Slate can refresh Linux font caches automatically.",
                )
            }
        }
    }
}

pub fn user_font_dir(env: &SlateEnv) -> PathBuf {
    user_font_dir_for_backend(env, backend())
}

fn user_font_dir_for_backend(env: &SlateEnv, backend: FontPlatformBackend) -> PathBuf {
    match backend {
        FontPlatformBackend::Macos => env.home().join("Library/Fonts"),
        FontPlatformBackend::Fontconfig => env.xdg_data_home().join("fonts"),
    }
}

pub fn font_search_paths(env: &SlateEnv) -> Vec<PathBuf> {
    font_search_paths_for_backend(env, backend())
}

fn font_search_paths_for_backend(env: &SlateEnv, backend: FontPlatformBackend) -> Vec<PathBuf> {
    let mut paths = vec![user_font_dir_for_backend(env, backend)];

    match backend {
        FontPlatformBackend::Macos => {
            paths.push(PathBuf::from("/Library/Fonts"));
            paths.push(PathBuf::from("/System/Library/Fonts"));
            paths.push(PathBuf::from(
                "/System/Applications/Utilities/Terminal.app/Contents/Resources/Fonts",
            ));
        }
        FontPlatformBackend::Fontconfig => {
            paths.push(env.home().join(".fonts"));
            paths.push(PathBuf::from("/usr/local/share/fonts"));
            paths.push(PathBuf::from("/usr/share/fonts"));
        }
    }

    paths
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_backend_label_is_stable() {
        assert_eq!(FontPlatformBackend::Macos.label(), "macOS fonts");
        assert_eq!(FontPlatformBackend::Fontconfig.label(), "fontconfig");
    }

    #[test]
    fn test_font_search_paths_for_fontconfig_include_user_and_system_locations() {
        let tempdir = tempfile::TempDir::new().unwrap();
        let env = SlateEnv::with_home(tempdir.path().to_path_buf());
        let paths = font_search_paths_for_backend(&env, FontPlatformBackend::Fontconfig);

        assert!(paths
            .iter()
            .any(|path| path.ends_with(".local/share/fonts")));
        assert!(paths.iter().any(|path| path.ends_with(".fonts")));
        assert!(paths
            .iter()
            .any(|path| path == &PathBuf::from("/usr/share/fonts")));
    }

    #[test]
    fn test_user_font_dir_for_fontconfig_uses_xdg_convention() {
        let tempdir = tempfile::TempDir::new().unwrap();
        let env = SlateEnv::with_home(tempdir.path().to_path_buf());

        assert_eq!(
            user_font_dir_for_backend(&env, FontPlatformBackend::Fontconfig),
            tempdir.path().join(".local/share/fonts")
        );
    }
}
