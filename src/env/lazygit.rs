//! Capture native config paths once; isolated profiles ignore host overrides.
use std::{
    ffi::OsString,
    path::{Path, PathBuf},
};

#[derive(Clone)]
pub(super) struct Paths {
    pub default: PathBuf,
    pub primary: PathBuf,
    pub selection: Option<OsString>,
}

impl Paths {
    pub fn capture(
        home: &Path,
        xdg: &Path,
        vars: &impl Fn(&str) -> Option<OsString>,
        isolated: bool,
    ) -> Self {
        // Lazygit's native macOS default differs from Slate's .config default.
        let default = if isolated {
            xdg.join("lazygit/config.yml")
        } else if let Some(root) = vars("XDG_CONFIG_HOME").filter(|v| !v.is_empty()) {
            PathBuf::from(root).join("lazygit/config.yml")
        } else if cfg!(target_os = "macos") {
            home.join("Library/Application Support/lazygit/config.yml")
        } else {
            xdg.join("lazygit/config.yml")
        };
        let selection = if isolated {
            None
        } else {
            vars("LG_CONFIG_FILE").filter(|value| !value.is_empty())
        };
        let primary = selection
            .as_ref()
            .and_then(|value| {
                // Comma is the native separator. Colons and whitespace are
                // legal filename bytes, not an alternate path-list grammar.
                use std::os::unix::ffi::{OsStrExt, OsStringExt};
                value
                    .as_bytes()
                    .split(|byte| *byte == b',')
                    .find(|part| !part.is_empty())
                    .map(|part| PathBuf::from(OsString::from_vec(part.to_vec())))
            })
            .unwrap_or_else(|| default.clone());
        Self {
            default,
            primary,
            selection,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lazygit_paths_capture_native_default_override_and_isolation() {
        let home = Path::new("/tmp/private-home");
        let xdg = home.join(".config");
        let plain = Paths::capture(home, &xdg, &|_| None, false);
        assert_eq!(
            plain.default,
            if cfg!(target_os = "macos") {
                home.join("Library/Application Support/lazygit/config.yml")
            } else {
                xdg.join("lazygit/config.yml")
            }
        );
        let vars = |name: &str| match name {
            "XDG_CONFIG_HOME" => Some(OsString::from("/tmp/custom config")),
            "LG_CONFIG_FILE" => Some(OsString::from("/tmp/colon:name .yml,/tmp/second.yml")),
            _ => None,
        };
        let custom = Paths::capture(home, &xdg, &vars, false);
        assert_eq!(
            custom.default,
            Path::new("/tmp/custom config/lazygit/config.yml")
        );
        assert_eq!(custom.primary, Path::new("/tmp/colon:name .yml"));
        assert_eq!(custom.selection, vars("LG_CONFIG_FILE"));
        let isolated = Paths::capture(home, &xdg, &vars, true);
        assert_eq!(isolated.default, xdg.join("lazygit/config.yml"));
        assert_eq!(isolated.primary, isolated.default);
        assert!(isolated.selection.is_none());
    }
}
