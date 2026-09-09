use crate::error::Result;
#[cfg(not(target_os = "linux"))]
use crate::error::SlateError;
use std::path::Path;

#[cfg(any(target_os = "linux", test))]
mod interfaces;
#[cfg(any(target_os = "linux", test))]
mod query_deadline;
#[cfg(any(target_os = "linux", test))]
mod screenshot;
#[cfg(any(target_os = "linux", test))]
mod screenshot_file;
#[cfg(any(target_os = "linux", test))]
mod settings;
#[cfg(test)]
pub(crate) mod test_bus;
#[cfg(any(target_os = "linux", test))]
pub(crate) mod watch;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PortalCaptureStatus {
    Captured,
    Cancelled,
}

#[cfg(target_os = "linux")]
mod imp {
    use super::{Path, PortalCaptureStatus, Result};

    pub fn settings_available() -> bool {
        settings_version().is_ok()
    }

    pub fn screenshot_available() -> bool {
        screenshot_version().is_ok()
    }

    pub fn settings_version() -> Result<u32> {
        super::settings::version()
    }

    pub fn screenshot_version() -> Result<u32> {
        super::screenshot::version()
    }

    pub fn read_color_scheme() -> Result<Option<u32>> {
        super::settings::read_color_scheme()
    }

    pub fn watch_color_scheme_changes<F>(on_change: F) -> Result<()>
    where
        F: FnMut(u32) -> Result<()>,
    {
        async_io::block_on(super::watch::run(
            zbus::Connection::session(),
            std::future::pending(),
            || Ok(()),
            on_change,
        ))
    }

    pub fn take_interactive_screenshot(output_path: &Path) -> Result<PortalCaptureStatus> {
        super::screenshot::capture(output_path)
    }
}

#[cfg(not(target_os = "linux"))]
mod imp {
    use super::{Path, PortalCaptureStatus, Result, SlateError};

    pub fn settings_available() -> bool {
        false
    }

    pub fn screenshot_available() -> bool {
        false
    }

    pub fn settings_version() -> Result<u32> {
        Err(SlateError::PlatformError(
            "XDG desktop portal settings are only available on Linux.".to_string(),
        ))
    }

    pub fn screenshot_version() -> Result<u32> {
        Err(SlateError::PlatformError(
            "XDG desktop portal screenshot capture is only available on Linux.".to_string(),
        ))
    }

    pub fn read_color_scheme() -> Result<Option<u32>> {
        Ok(None)
    }

    pub fn watch_color_scheme_changes<F>(_on_change: F) -> Result<()>
    where
        F: FnMut(u32) -> Result<()>,
    {
        Err(SlateError::PlatformError(
            "XDG desktop portal settings are only available on Linux.".to_string(),
        ))
    }

    pub fn take_interactive_screenshot(_output_path: &Path) -> Result<PortalCaptureStatus> {
        Err(SlateError::PlatformError(
            "XDG desktop portal screenshot capture is only available on Linux.".to_string(),
        ))
    }
}

pub fn settings_available() -> bool {
    imp::settings_available()
}

pub fn screenshot_available() -> bool {
    imp::screenshot_available()
}

pub fn settings_version() -> Result<u32> {
    imp::settings_version()
}

pub fn screenshot_version() -> Result<u32> {
    imp::screenshot_version()
}

pub fn read_color_scheme() -> Result<Option<u32>> {
    imp::read_color_scheme()
}

pub fn watch_color_scheme_changes<F>(on_change: F) -> Result<()>
where
    F: FnMut(u32) -> Result<()>,
{
    imp::watch_color_scheme_changes(on_change)
}

pub fn take_interactive_screenshot(output_path: &Path) -> Result<PortalCaptureStatus> {
    imp::take_interactive_screenshot(output_path)
}
