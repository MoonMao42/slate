//! Capture custom TUI paths once so apply and recovery cannot reinterpret them
//! after a cwd/environment change. Never dereference the final file symlink.
use std::ffi::OsString;
use std::path::{Component, Path, PathBuf};

#[derive(Clone)]
pub(super) struct TuiConfig {
    pub(super) path: PathBuf,
    pub(super) was_relative: bool,
    pub(super) error: Option<String>,
}

fn invalid(reason: &str) -> String {
    format!("OPENCODE_TUI_CONFIG: {reason}; no fallback path was selected")
}

impl TuiConfig {
    pub(super) fn capture(value: Option<OsString>, isolated: bool) -> Option<Self> {
        if isolated {
            return None;
        }
        let value = value.filter(|value| {
            !value.is_empty() && value.to_str().is_none_or(|s| !s.trim().is_empty())
        })?;
        let input = PathBuf::from(value);
        let was_relative = input.is_relative();
        let absolute = if was_relative {
            match std::env::current_dir() {
                Ok(cwd) => cwd.join(&input),
                Err(_) => {
                    return Some(Self {
                        path: input,
                        was_relative,
                        error: Some(invalid(
                            "cannot resolve the current directory for a relative path",
                        )),
                    })
                }
            }
        } else {
            input
        };
        // Keep an invalid explicit target visible, without breaking unrelated
        // profile commands or pretending a default path was selected. Writers
        // validate this captured error before attempting the OpenCode edit.
        let (path, error) = match resolve(&absolute) {
            Ok(path) => (path, None),
            Err(error) => (absolute, Some(error)),
        };
        Some(Self {
            path,
            was_relative,
            error,
        })
    }
}

fn resolve(absolute: &Path) -> std::result::Result<PathBuf, String> {
    // Discarding a trailing slash or '/.' could turn an invalid directory
    // reference into an existing regular file and authorize overwriting it.
    let raw = absolute.as_os_str().as_encoded_bytes();
    if raw.ends_with(b"/") || raw.ends_with(b"/.") {
        return Err(invalid(
            "the override must name a file, not end in '/' or '/.'",
        ));
    }
    let mut path = PathBuf::new();
    for component in absolute.components() {
        match component {
            Component::ParentDir => {
                // Lexically popping link/.. would select the wrong directory.
                // The traversed prefix must exist, just as in an OS path lookup;
                // a missing component before '..' is never invented or skipped.
                let resolved = std::fs::canonicalize(&path)
                    .map_err(|_| invalid("cannot resolve the directory before '..'"))?;
                if !resolved.is_dir() {
                    return Err(invalid("the component before '..' is not a directory"));
                }
                path = resolved;
                path.pop(); // At filesystem root, '..' still denotes root.
            }
            Component::CurDir => {}
            other => path.push(other.as_os_str()),
        }
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::env::SlateEnv;
    use std::fs;
    use std::os::unix::fs::symlink;

    #[test]
    fn opencode_override_is_captured_from_injected_vars_and_ignored_when_isolated() {
        let td = tempfile::tempdir().unwrap();
        let explicit = td.path().join("custom/tui.jsonc");
        let env = SlateEnv::from_vars(|name| match name {
            "HOME" => Some(td.path().as_os_str().to_owned()),
            "OPENCODE_TUI_CONFIG" => Some(explicit.as_os_str().to_owned()),
            _ => None,
        })
        .unwrap();
        assert_eq!(env.opencode_tui_config(), Some(explicit.as_path()));
        assert!(!env.opencode_tui_config_was_relative());
        assert_eq!(env.clone().opencode_tui_config(), env.opencode_tui_config());
        let isolated = SlateEnv::from_vars(|name| match name {
            "HOME" | "SLATE_HOME" => Some(td.path().as_os_str().to_owned()),
            "OPENCODE_TUI_CONFIG" => Some("unresolvable/../PRIVATE_CONTENT.json".into()),
            _ => None,
        })
        .unwrap();
        assert!(isolated.opencode_tui_config().is_none());
        assert!(SlateEnv::with_home(td.path().to_owned())
            .opencode_tui_config()
            .is_none());
        assert_eq!(fs::read_dir(td.path()).unwrap().count(), 0);
    }

    #[test]
    fn opencode_override_resolves_parent_directories_without_following_final_links() {
        let td = tempfile::tempdir().unwrap();
        let physical = fs::canonicalize(td.path()).unwrap();
        fs::create_dir_all(td.path().join("real/child")).unwrap();
        symlink(td.path().join("real/child"), td.path().join("alias")).unwrap();
        // OS meaning is real/tui.jsonc, not the lexical sibling tui.jsonc.
        assert_eq!(
            resolve(&td.path().join("alias/../tui.jsonc")).unwrap(),
            physical.join("real/tui.jsonc")
        );
        assert_eq!(
            resolve(&td.path().join("real/../new/tui.jsonc")).unwrap(),
            physical.join("new/tui.jsonc")
        );
        let final_link = td.path().join("final.jsonc");
        symlink(td.path().join("missing"), &final_link).unwrap();
        assert_eq!(resolve(&final_link).unwrap(), final_link);
        assert!(resolve(&td.path().join("missing/../PRIVATE_CONTENT.json")).is_err());
        for suffix in ["regular/", "regular/."] {
            let error = resolve(&td.path().join(suffix)).unwrap_err().to_string();
            assert!(!error.contains("PRIVATE_CONTENT"));
        }
    }
}
