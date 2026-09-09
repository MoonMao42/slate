//! One resolved executable, one bounded version probe, no configuration writes.
use crate::detection::{ToolEvidence, ToolPresence};
use crate::env::SlateEnv;
use crate::error::{Result, SlateError};
use crate::platform::version_check::{detect_version_at, VersionPolicy};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NvimAvailability {
    Missing,
    Unsupported,
    Ready,
}

/// Keep evidence and the parsed result together for explicit diagnostics. A
/// missing executable is distinct from a failed check of a resolved executable.
pub(crate) struct NvimProbe {
    pub binary: Option<PathBuf>,
    pub in_path: bool,
    pub version: Result<Option<semver::Version>>,
}

impl NvimProbe {
    fn into_availability(self) -> Result<NvimAvailability> {
        Ok(match self.version? {
            None => NvimAvailability::Missing,
            Some(version) if VersionPolicy::supports("nvim", &version) => NvimAvailability::Ready,
            Some(_) => NvimAvailability::Unsupported,
        })
    }
}

pub(crate) fn detect(env: &SlateEnv) -> Result<NvimAvailability> {
    inspect(env).into_availability()
}

/// Explicit native check; do not call from file-only diagnostics.
pub(crate) fn inspect(env: &SlateEnv) -> NvimProbe {
    let presence = crate::detection::detect_tool_presence_with_env("nvim", env);
    if !presence.installed {
        if let Some(candidate) = crate::detection::unusable_command_with_env("nvim", env) {
            return from_unusable(candidate);
        }
    }
    from_presence(presence, |executable| detect_version_at("nvim", executable))
}

fn from_presence(
    presence: ToolPresence,
    probe: impl FnOnce(&Path) -> Result<semver::Version>,
) -> NvimProbe {
    let binary = match presence.evidence {
        Some(ToolEvidence::Executable(path)) => Some(path),
        _ => None,
    };
    let version = if !presence.installed {
        Ok(None)
    } else if let Some(executable) = &binary {
        probe(executable).map(Some)
    } else {
        Err(SlateError::PlatformError(
            "Neovim detection did not resolve an executable; version was not checked".into(),
        ))
    };
    NvimProbe {
        binary,
        in_path: presence.in_path,
        version,
    }
}

fn from_unusable(candidate: crate::detection::UnusableCommand) -> NvimProbe {
    NvimProbe {
        binary: Some(candidate.path),
        in_path: candidate.in_path,
        version: Err(SlateError::PlatformError(
            "Neovim candidate is not accessible as an executable regular file; check permissions, file type and link target. No version command was started. Use `slate doctor nvim --check-version` to inspect the candidate path".into(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::ffi::OsStrExt;

    #[test]
    fn executable_lookup_unusable_nvim_is_an_error_with_candidate_evidence() {
        let path = PathBuf::from("/private/blocked/nvim");
        let probe = from_unusable(crate::detection::UnusableCommand {
            path: path.clone(),
            in_path: true,
        });
        assert_eq!(probe.binary, Some(path));
        assert!(probe.in_path);
        let error = probe.into_availability().unwrap_err().to_string();
        assert!(error.contains("No version command was started"));
        assert!(error.contains("doctor nvim --check-version"));
    }

    #[test]
    fn nvim_availability_distinguishes_missing_old_ready_and_probe_failure() {
        assert_eq!(
            from_presence(ToolPresence::missing(), |_| panic!("missing tool probed"))
                .into_availability()
                .unwrap(),
            NvimAvailability::Missing
        );
        // No filesystem fixture is required to prove the resolved argument
        // preserves non-UTF-8 bytes (APFS cannot create such filenames).
        let path = Path::new(std::ffi::OsStr::from_bytes(
            b"/private/resolved-\xff editor/nvim",
        ));
        for (version, expected) in [
            ("0.7.2", NvimAvailability::Unsupported),
            ("0.8.0", NvimAvailability::Ready),
            ("0.12.0", NvimAvailability::Ready),
        ] {
            let presence = ToolPresence::fallback_with(ToolEvidence::Executable(path.into()));
            let actual = from_presence(presence, |executable| {
                assert_eq!(executable, path);
                Ok(version.parse().unwrap())
            })
            .into_availability()
            .unwrap();
            assert_eq!(actual, expected);
        }
        let presence = ToolPresence::in_path_with(ToolEvidence::Executable(path.into()));
        let error = from_presence(presence, |_| {
            Err(SlateError::PlatformError("probe failed".into()))
        })
        .into_availability()
        .unwrap_err();
        assert!(error.to_string().contains("probe failed"));
        assert!(from_presence(
            ToolPresence::installed_with(ToolEvidence::Config(path.into())),
            |_| panic!("non-executable evidence probed")
        )
        .into_availability()
        .is_err());
    }
}
