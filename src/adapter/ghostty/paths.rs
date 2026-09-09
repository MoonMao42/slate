//! Default-file order from Ghostty 1.3.1 Config.loadDefaultFiles. Labels and
//! recovery keys travel with paths so reordering cannot relabel backup contents.
use std::path::{Path, PathBuf};

pub(crate) struct ConfigCandidate {
    pub key: &'static str,
    pub label: &'static str,
    pub path: PathBuf,
}

pub(super) fn candidates(xdg_dir: &Path, home: Option<&Path>) -> Vec<ConfigCandidate> {
    candidates_for_platform(xdg_dir, home, cfg!(target_os = "macos"))
}

fn candidates_for_platform(
    xdg_dir: &Path,
    home: Option<&Path>,
    macos: bool,
) -> Vec<ConfigCandidate> {
    let mut entries = vec![
        ConfigCandidate {
            key: "ghostty-xdg-config",
            label: "XDG config",
            path: xdg_dir.join("config"),
        },
        ConfigCandidate {
            key: "ghostty-xdg-config-ghostty",
            label: "XDG config.ghostty",
            path: xdg_dir.join("config.ghostty"),
        },
    ];
    if let Some(home) = home.filter(|_| macos) {
        let directory = home.join("Library/Application Support/com.mitchellh.ghostty");
        entries.extend([
            ConfigCandidate {
                key: "ghostty-macos-app-support-config",
                label: "macOS App Support config",
                path: directory.join("config"),
            },
            ConfigCandidate {
                key: "ghostty-macos-app-support-config-ghostty",
                label: "macOS App Support config.ghostty",
                path: directory.join("config.ghostty"),
            },
        ]);
    }
    entries
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ghostty_entry_order_binds_stable_keys_and_labels_on_both_platforms() {
        let xdg = Path::new("/profile/xdg/ghostty");
        for macos in [false, true] {
            let entries = candidates_for_platform(xdg, Some(Path::new("/profile")), macos);
            let expected = [
                (
                    "ghostty-xdg-config",
                    "XDG config",
                    "/profile/xdg/ghostty/config",
                ),
                (
                    "ghostty-xdg-config-ghostty",
                    "XDG config.ghostty",
                    "/profile/xdg/ghostty/config.ghostty",
                ),
                (
                    "ghostty-macos-app-support-config",
                    "macOS App Support config",
                    "/profile/Library/Application Support/com.mitchellh.ghostty/config",
                ),
                (
                    "ghostty-macos-app-support-config-ghostty",
                    "macOS App Support config.ghostty",
                    "/profile/Library/Application Support/com.mitchellh.ghostty/config.ghostty",
                ),
            ];
            assert_eq!(entries.len(), if macos { 4 } else { 2 });
            for (entry, (key, label, path)) in entries.iter().zip(expected) {
                assert_eq!(
                    (entry.key, entry.label, entry.path.as_path()),
                    (key, label, Path::new(path))
                );
            }
        }
        assert_eq!(candidates_for_platform(xdg, None, true).len(), 2);
    }

    #[cfg(unix)]
    #[test]
    fn ghostty_candidate_paths_preserve_non_unicode_home_bytes() {
        use std::{ffi::OsString, os::unix::ffi::OsStringExt};
        let home = PathBuf::from(OsString::from_vec(b"/profile-\xff".to_vec()));
        let xdg = home.join("xdg/ghostty");
        for macos in [false, true] {
            let entries = candidates_for_platform(&xdg, Some(&home), macos);
            assert_eq!(entries.len(), if macos { 4 } else { 2 });
            assert_eq!(entries[0].path, xdg.join("config"));
            assert_eq!(entries[1].path, xdg.join("config.ghostty"));
            if macos {
                assert_eq!(
                    entries[2].path,
                    home.join("Library/Application Support/com.mitchellh.ghostty/config")
                );
                assert_eq!(
                    entries[3].path,
                    home.join("Library/Application Support/com.mitchellh.ghostty/config.ghostty")
                );
            }
            assert!(entries.iter().all(|entry| entry.path.to_str().is_none()));
        }
    }
}
