//! Read-only installation inspection. Never execute a launcher or expose bytes.
use super::{launcher_contents, SlateEnv, HELPER, LAUNCHER};
use serde::Serialize;
use std::fs::{self, OpenOptions};
use std::io::Read;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InstallationState {
    Current,
    Missing,
    Outdated,
    Legacy,
    Unrecognized,
    Unsafe,
    Unreadable,
}

impl InstallationState {
    pub fn label(self) -> &'static str {
        match self {
            Self::Current => "current",
            Self::Missing => "missing",
            Self::Outdated => "outdated or locally changed",
            Self::Legacy => "legacy artifact format",
            Self::Unrecognized => "unrecognized content",
            Self::Unsafe => "unsafe file type",
            Self::Unreadable => "unreadable",
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ComponentInspection {
    pub path: PathBuf,
    pub state: InstallationState,
    pub executable: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct InstallationInspection {
    pub directory: PathBuf,
    pub directory_access: DirectoryAccess,
    pub launcher: ComponentInspection,
    pub helper: Option<ComponentInspection>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DirectoryAccess {
    Allowed,
    Denied,
    Missing,
    NotDirectory,
    Unknown,
}

impl DirectoryAccess {
    pub fn label(self) -> &'static str {
        match self {
            Self::Allowed => "write/search access allowed (not a write guarantee)",
            Self::Denied => "write/search access denied",
            Self::Missing => "not created; access not checked",
            Self::NotDirectory => "path is not a directory",
            Self::Unknown => "access could not be determined",
        }
    }
}

fn directory_access(path: &Path) -> DirectoryAccess {
    use std::os::unix::ffi::OsStrExt;
    match fs::metadata(path) {
        Ok(meta) if !meta.is_dir() => return DirectoryAccess::NotDirectory,
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return DirectoryAccess::Missing
        }
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
            return DirectoryAccess::Denied
        }
        Err(_) => return DirectoryAccess::Unknown,
    }
    let Ok(path) = std::ffi::CString::new(path.as_os_str().as_bytes()) else {
        return DirectoryAccess::Unknown;
    };
    // Read-only OS check using real IDs, including ACL decisions. No probe
    // file is created; allowed does not guarantee a later atomic replacement.
    if unsafe { libc::access(path.as_ptr(), libc::W_OK | libc::X_OK) } == 0 {
        DirectoryAccess::Allowed
    } else if std::io::Error::last_os_error().kind() == std::io::ErrorKind::PermissionDenied {
        DirectoryAccess::Denied
    } else {
        DirectoryAccess::Unknown
    }
}

fn inspect_asset(
    path: &Path,
    classify: impl FnOnce(&[u8]) -> InstallationState,
) -> ComponentInspection {
    let mut report = ComponentInspection {
        path: path.to_owned(),
        state: InstallationState::Unreadable,
        executable: None,
    };
    match fs::symlink_metadata(path) {
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            report.state = InstallationState::Missing;
            return report;
        }
        Err(_) => return report,
        Ok(meta) if !meta.is_file() => {
            report.state = InstallationState::Unsafe;
            return report;
        }
        Ok(_) => {}
    }
    let Ok(file) = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK)
        .open(path)
    else {
        return report;
    };
    let Ok(meta) = file.metadata() else {
        return report;
    };
    if !meta.is_file() {
        report.state = InstallationState::Unsafe;
        return report;
    }
    report.executable = Some(meta.permissions().mode() & 0o111 != 0);
    let mut bytes = Vec::new();
    // Enough for the small native event helper and shell launcher, bounded for
    // an unrelated binary accidentally placed at one of these paths.
    if file.take(1_048_577).read_to_end(&mut bytes).is_err() {
        return report;
    }
    if bytes.len() > 1_048_576 {
        report.state = InstallationState::Unrecognized;
        return report;
    }
    report.state = classify(&bytes);
    report
}

pub fn inspect_installation(env: &SlateEnv) -> InstallationInspection {
    let directory = env.config_dir().join("managed/bin");
    let expected = launcher_contents(env).ok();
    let launcher = inspect_asset(&directory.join(LAUNCHER), |bytes| {
        if expected
            .as_ref()
            .is_some_and(|expected| expected.as_bytes() == bytes)
        {
            return InstallationState::Current;
        }
        let text = std::str::from_utf8(bytes).ok();
        if text.is_some_and(|text| text.starts_with("#!/bin/sh\n# Slate watcher launcher v1\n")) {
            return InstallationState::Outdated;
        }
        if expected.as_ref().is_some_and(|expected| {
            expected
                .replacen("# Slate watcher launcher v1\n", "", 1)
                .as_bytes()
                == bytes
        }) {
            return InstallationState::Outdated;
        }
        // This identifies an old artifact format, not a running legacy process.
        if [
            b"\x7fELF".as_slice(),
            b"\xcf\xfa\xed\xfe",
            b"\xce\xfa\xed\xfe",
            b"\xca\xfe\xba\xbe",
        ]
        .iter()
        .any(|magic| bytes.starts_with(magic))
            || text
                .is_some_and(|text| text.starts_with("#!") && text.contains("__watch-auto-theme"))
        {
            return InstallationState::Legacy;
        }
        InstallationState::Unrecognized
    });
    #[cfg(target_os = "macos")]
    let helper = Some(inspect_asset(&directory.join(HELPER), |bytes| {
        if !super::EMBEDDED_WATCHER.is_empty() && bytes == super::EMBEDDED_WATCHER {
            InstallationState::Current
        } else {
            InstallationState::Outdated
        }
    }));
    #[cfg(not(target_os = "macos"))]
    let helper = {
        let _ = HELPER;
        None
    };
    InstallationInspection {
        directory_access: directory_access(&directory),
        directory,
        launcher,
        helper,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn watcher_directory_access_distinguishes_existing_missing_and_nondirectory_without_writes() {
        let root = tempfile::tempdir().unwrap();
        assert_eq!(directory_access(root.path()), DirectoryAccess::Allowed);
        assert_eq!(
            directory_access(&root.path().join("absent")),
            DirectoryAccess::Missing
        );
        let file = root.path().join("file");
        fs::write(&file, b"sentinel").unwrap();
        assert_eq!(directory_access(&file), DirectoryAccess::NotDirectory);
        assert_eq!(fs::read_dir(root.path()).unwrap().count(), 1);
        assert_eq!(fs::read(file).unwrap(), b"sentinel");
    }

    #[test]
    #[ignore = "explicit blocked directory required; read-only access check"]
    fn watcher_directory_access_reports_native_denial_without_writes() {
        let path = std::env::var_os("SLATE_INSPECT_BLOCKED_DIRECTORY").expect("provide directory");
        assert_eq!(directory_access(Path::new(&path)), DirectoryAccess::Denied);
    }
}
