use crate::error::{Result, SlateError};
use crate::platform::capabilities::{CapabilityReport, SupportLevel};
use std::path::Path;
use std::process::{Command, Stdio};

pub(crate) mod image_file;
use image_file::CapturedImage;

const GNOME_FALLBACK_REASON: &str =
    "XDG desktop portal screenshot capture was unavailable, so Slate fell back to GNOME screenshot.";
const UNSUPPORTED_CAPTURE_REASON: &str =
    "No supported screenshot backend was found. Share URI export is still available.";
const GNOME_MISSING_REASON: &str = "GNOME screenshot fallback requires gnome-screenshot.";
const GNOME_CANCELLED_REASON: &str = "GNOME screenshot fallback was cancelled.";
const MACOS_CANCELLED_REASON: &str = "Screenshot cancelled or failed.";
const PORTAL_CANCELLED_REASON: &str = "Portal screenshot capture was cancelled.";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShareCaptureBackend {
    MacosScreenCapture,
    XdgDesktopPortal,
    GnomeScreenshot,
    Unsupported,
}

impl ShareCaptureBackend {
    pub fn label(self) -> &'static str {
        match self {
            Self::MacosScreenCapture => "macOS screencapture",
            Self::XdgDesktopPortal => "XDG desktop portal",
            Self::GnomeScreenshot => "GNOME screenshot",
            Self::Unsupported => "unsupported",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShareCaptureResult {
    pub captured: bool,
    pub reason: Option<String>,
}

pub fn detect_backend() -> ShareCaptureBackend {
    if cfg!(target_os = "macos") {
        return ShareCaptureBackend::MacosScreenCapture;
    }

    if cfg!(target_os = "linux") && crate::platform::portal::screenshot_available() {
        return ShareCaptureBackend::XdgDesktopPortal;
    }

    if cfg!(target_os = "linux")
        && crate::platform::desktop::is_gnome_session()
        && crate::detection::command_path("gnome-screenshot").is_some()
    {
        return ShareCaptureBackend::GnomeScreenshot;
    }

    ShareCaptureBackend::Unsupported
}

fn capability_report_for_backend(backend: ShareCaptureBackend) -> CapabilityReport {
    match backend {
        ShareCaptureBackend::MacosScreenCapture => {
            CapabilityReport::supported("macos-screencapture")
        }
        ShareCaptureBackend::XdgDesktopPortal => CapabilityReport::supported("xdg-desktop-portal"),
        ShareCaptureBackend::GnomeScreenshot => CapabilityReport {
            level: SupportLevel::BestEffort,
            backend: "gnome-screenshot",
            reason: Some(GNOME_FALLBACK_REASON.to_string()),
        },
        ShareCaptureBackend::Unsupported => {
            CapabilityReport::unsupported("unsupported", UNSUPPORTED_CAPTURE_REASON)
        }
    }
}

pub fn capability_report() -> CapabilityReport {
    capability_report_for_backend(detect_backend())
}

pub fn capture_interactive(output_path: &Path) -> Result<ShareCaptureResult> {
    match std::fs::symlink_metadata(output_path) {
        Ok(_) => {
            return Err(SlateError::PlatformError(
                "Screenshot output already exists; choose an unused path.".into(),
            ))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    match capture_draft()? {
        CaptureDraft::Captured(image) => {
            image.save_new(output_path)?;
            Ok(ShareCaptureResult {
                captured: true,
                reason: None,
            })
        }
        CaptureDraft::Unavailable(result) => Ok(result),
    }
}

pub(crate) enum CaptureDraft {
    Captured(CapturedImage),
    Unavailable(ShareCaptureResult),
}

pub(crate) fn capture_draft() -> Result<CaptureDraft> {
    draft_with(|path| capture_backend(detect_backend(), path))
}

fn draft_with(capture: impl FnOnce(&Path) -> Result<ShareCaptureResult>) -> Result<CaptureDraft> {
    let scratch = tempfile::Builder::new()
        .prefix("slate-capture-")
        .tempdir()?;
    let path = scratch.path().join("capture.png");
    let result = capture(&path)?;
    if !result.captured {
        return Ok(CaptureDraft::Unavailable(result));
    }
    match CapturedImage::read(&path)? {
        Some(image) => Ok(CaptureDraft::Captured(image)),
        None => Ok(CaptureDraft::Unavailable(ShareCaptureResult {
            captured: false,
            reason: Some(
                "Screenshot cancelled or backend produced no image; share URI is still available."
                    .into(),
            ),
        })),
    }
}

fn capture_backend(backend: ShareCaptureBackend, output_path: &Path) -> Result<ShareCaptureResult> {
    match backend {
        ShareCaptureBackend::MacosScreenCapture => {
            let binary = crate::detection::command_in_actual_path("screencapture")
                .or_else(|| crate::detection::command_path("screencapture"))
                .ok_or_else(|| {
                    SlateError::PlatformError(
                        "Screenshot backend screencapture is unavailable.".into(),
                    )
                })?;
            native_capture(
                &binary,
                &["-w", "-o", "-t", "png"],
                output_path,
                MACOS_CANCELLED_REASON,
            )
        }
        ShareCaptureBackend::XdgDesktopPortal => {
            match crate::platform::portal::take_interactive_screenshot(output_path)? {
                crate::platform::portal::PortalCaptureStatus::Captured => Ok(ShareCaptureResult {
                    captured: true,
                    reason: None,
                }),
                crate::platform::portal::PortalCaptureStatus::Cancelled => Ok(ShareCaptureResult {
                    captured: false,
                    reason: Some(PORTAL_CANCELLED_REASON.to_string()),
                }),
            }
        }
        ShareCaptureBackend::GnomeScreenshot => {
            let Some(command) = crate::detection::command_in_actual_path("gnome-screenshot")
                .or_else(|| crate::detection::command_path("gnome-screenshot"))
            else {
                return Ok(ShareCaptureResult {
                    captured: false,
                    reason: Some(GNOME_MISSING_REASON.to_string()),
                });
            };

            native_capture(&command, &["-a", "-f"], output_path, GNOME_CANCELLED_REASON)
        }
        ShareCaptureBackend::Unsupported => Ok(ShareCaptureResult {
            captured: false,
            reason: Some(UNSUPPORTED_CAPTURE_REASON.to_string()),
        }),
    }
}

fn native_capture(
    binary: &Path,
    args: &[&str],
    output: &Path,
    cancelled: &str,
) -> Result<ShareCaptureResult> {
    let status = Command::new(binary)
        .args(args)
        .arg(output)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|_| {
            SlateError::PlatformError(
                "Failed to launch screenshot backend; native output omitted.".into(),
            )
        })?;
    Ok(ShareCaptureResult {
        captured: status.success(),
        reason: (!status.success()).then(|| cancelled.into()),
    })
}

#[cfg(test)]
#[path = "share/capture_tests.rs"]
mod capture_tests;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_backend_labels() {
        assert_eq!(
            ShareCaptureBackend::MacosScreenCapture.label(),
            "macOS screencapture"
        );
        assert_eq!(
            ShareCaptureBackend::XdgDesktopPortal.label(),
            "XDG desktop portal"
        );
        assert_eq!(
            ShareCaptureBackend::GnomeScreenshot.label(),
            "GNOME screenshot"
        );
    }

    #[test]
    fn test_capability_report_for_portal_backend_is_supported() {
        let report = capability_report_for_backend(ShareCaptureBackend::XdgDesktopPortal);

        assert_eq!(report.level, SupportLevel::Supported);
        assert_eq!(report.backend, "xdg-desktop-portal");
    }

    #[test]
    fn test_capability_report_for_gnome_backend_uses_best_effort() {
        let report = capability_report_for_backend(ShareCaptureBackend::GnomeScreenshot);

        assert_eq!(report.level, SupportLevel::BestEffort);
        assert_eq!(report.backend, "gnome-screenshot");
        assert_eq!(report.reason.as_deref(), Some(GNOME_FALLBACK_REASON));
    }

    #[test]
    fn test_capability_report_for_unsupported_backend_preserves_share_uri_reason() {
        let report = capability_report_for_backend(ShareCaptureBackend::Unsupported);

        assert_eq!(report.level, SupportLevel::Unsupported);
        assert_eq!(report.backend, "unsupported");
        assert!(report
            .reason
            .as_deref()
            .unwrap_or_default()
            .contains("Share URI export is still available"));
    }
}
