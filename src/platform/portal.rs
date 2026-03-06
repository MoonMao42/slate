use crate::error::{Result, SlateError};
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PortalCaptureStatus {
    Captured,
    Cancelled,
}

#[cfg(target_os = "linux")]
mod imp {
    use super::{Path, PortalCaptureStatus, Result, SlateError};
    use std::collections::HashMap;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};
    use url::Url;
    use zbus::blocking::Connection;
    use zbus::proxy;
    use zbus::zvariant::{OwnedObjectPath, OwnedValue};

    const DESKTOP_SERVICE: &str = "org.freedesktop.portal.Desktop";
    const APPEARANCE_NAMESPACE: &str = "org.freedesktop.appearance";
    const COLOR_SCHEME_KEY: &str = "color-scheme";

    #[proxy(
        interface = "org.freedesktop.portal.Settings",
        default_service = "org.freedesktop.portal.Desktop",
        default_path = "/org/freedesktop/portal/desktop"
    )]
    trait PortalSettings {
        #[zbus(property)]
        fn version(&self) -> zbus::Result<u32>;

        #[zbus(name = "ReadOne")]
        fn read_one(&self, namespace: &str, key: &str) -> zbus::Result<OwnedValue>;

        #[zbus(signal)]
        fn setting_changed(
            &self,
            namespace: &str,
            key: &str,
            value: OwnedValue,
        ) -> zbus::Result<()>;
    }

    #[proxy(
        interface = "org.freedesktop.portal.Screenshot",
        default_service = "org.freedesktop.portal.Desktop",
        default_path = "/org/freedesktop/portal/desktop"
    )]
    trait PortalScreenshot {
        #[zbus(property)]
        fn version(&self) -> zbus::Result<u32>;

        fn screenshot(
            &self,
            parent_window: &str,
            options: HashMap<&str, OwnedValue>,
        ) -> zbus::Result<OwnedObjectPath>;
    }

    #[proxy(interface = "org.freedesktop.portal.Request")]
    trait PortalRequest {
        #[zbus(signal)]
        fn response(&self, response: u32, results: HashMap<String, OwnedValue>)
            -> zbus::Result<()>;
    }

    fn session_connection() -> Result<Connection> {
        Connection::session().map_err(|err| {
            SlateError::PlatformError(format!(
                "Failed to connect to the session D-Bus for portal access: {}",
                err
            ))
        })
    }

    fn settings_proxy<'a>(connection: &'a Connection) -> Result<PortalSettingsProxyBlocking<'a>> {
        PortalSettingsProxyBlocking::new(connection).map_err(|err| {
            SlateError::PlatformError(format!(
                "Failed to connect to the XDG desktop portal settings backend: {}",
                err
            ))
        })
    }

    fn screenshot_proxy<'a>(
        connection: &'a Connection,
    ) -> Result<PortalScreenshotProxyBlocking<'a>> {
        PortalScreenshotProxyBlocking::new(connection).map_err(|err| {
            SlateError::PlatformError(format!(
                "Failed to connect to the XDG desktop portal screenshot backend: {}",
                err
            ))
        })
    }

    fn request_proxy<'a>(
        connection: &'a Connection,
        path: OwnedObjectPath,
    ) -> Result<PortalRequestProxyBlocking<'a>> {
        PortalRequestProxyBlocking::builder(connection)
            .destination(DESKTOP_SERVICE)
            .map_err(portal_error)?
            .path(path)
            .map_err(portal_error)?
            .build()
            .map_err(portal_error)
    }

    fn portal_error<E: std::fmt::Display>(err: E) -> SlateError {
        SlateError::PlatformError(format!("Portal request failed: {}", err))
    }

    fn portal_handle_token(prefix: &str) -> String {
        let millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        format!("slate_{}_{}_{}", prefix, std::process::id(), millis)
    }

    fn owned_value_to_u32(value: &OwnedValue) -> Result<u32> {
        let cloned = value.try_clone().map_err(portal_error)?;
        u32::try_from(cloned).map_err(|_| {
            SlateError::PlatformError("Portal value did not contain the expected integer.".into())
        })
    }

    fn owned_value_to_string(value: &OwnedValue) -> Result<String> {
        let cloned = value.try_clone().map_err(portal_error)?;
        String::try_from(cloned).map_err(|_| {
            SlateError::PlatformError("Portal value did not contain the expected string.".into())
        })
    }

    fn file_uri_to_path(uri: &str) -> Result<PathBuf> {
        let url = Url::parse(uri).map_err(|err| {
            SlateError::PlatformError(format!(
                "Portal screenshot returned an invalid URI: {}",
                err
            ))
        })?;
        url.to_file_path().map_err(|_| {
            SlateError::PlatformError(
                "Portal screenshot did not return a local file URI.".to_string(),
            )
        })
    }

    pub fn settings_available() -> bool {
        settings_version().is_ok()
    }

    pub fn screenshot_available() -> bool {
        screenshot_version().is_ok()
    }

    pub fn settings_version() -> Result<u32> {
        let connection = session_connection()?;
        let proxy = settings_proxy(&connection)?;
        proxy.version().map_err(portal_error)
    }

    pub fn screenshot_version() -> Result<u32> {
        let connection = session_connection()?;
        let proxy = screenshot_proxy(&connection)?;
        proxy.version().map_err(portal_error)
    }

    pub fn read_color_scheme() -> Result<Option<u32>> {
        let connection = session_connection()?;
        let proxy = settings_proxy(&connection)?;
        let value = proxy
            .read_one(APPEARANCE_NAMESPACE, COLOR_SCHEME_KEY)
            .map_err(portal_error)?;
        Ok(Some(owned_value_to_u32(&value)?))
    }

    pub fn watch_color_scheme_changes<F>(mut on_change: F) -> Result<()>
    where
        F: FnMut(u32) -> Result<()>,
    {
        let connection = session_connection()?;
        let proxy = settings_proxy(&connection)?;
        let mut changes = proxy.receive_setting_changed().map_err(portal_error)?;

        while let Some(signal) = changes.next() {
            let args = signal.args().map_err(portal_error)?;
            if *args.namespace() != APPEARANCE_NAMESPACE || *args.key() != COLOR_SCHEME_KEY {
                continue;
            }
            let scheme = owned_value_to_u32(args.value())?;
            on_change(scheme)?;
        }

        // Stream closed — the portal or D-Bus session went away. Shell integration will
        // relaunch us on next shell start, but surface the exit so users inspecting
        // `ps` / systemd journal can tell the watcher died rather than being silently
        // dormant.
        eprintln!(
            "slate: portal color-scheme signal stream closed; auto-theme watcher exiting"
        );
        Ok(())
    }

    pub fn take_interactive_screenshot(output_path: &Path) -> Result<PortalCaptureStatus> {
        if let Some(parent) = output_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let connection = session_connection()?;
        let proxy = screenshot_proxy(&connection)?;
        let version = proxy.version().map_err(portal_error)?;
        let handle_token = portal_handle_token("shot");

        let mut options = HashMap::new();
        options.insert(
            "handle_token",
            OwnedValue::try_from(zbus::zvariant::Value::from(handle_token))
                .map_err(portal_error)?,
        );
        if version >= 2 {
            options.insert("interactive", true.into());
        }

        let handle = proxy.screenshot("", options).map_err(portal_error)?;
        let request = request_proxy(&connection, handle)?;
        let mut responses = request.receive_response().map_err(portal_error)?;
        let Some(signal) = responses.next() else {
            return Err(SlateError::PlatformError(
                "Portal screenshot request did not return a response.".to_string(),
            ));
        };

        let args = signal.args().map_err(portal_error)?;
        match args.response() {
            0 => {
                let uri = args
                    .results()
                    .get("uri")
                    .ok_or_else(|| {
                        SlateError::PlatformError(
                            "Portal screenshot response did not include a URI.".to_string(),
                        )
                    })
                    .and_then(owned_value_to_string)?;
                let source_path = file_uri_to_path(&uri)?;
                fs::copy(&source_path, output_path).map_err(|err| {
                    SlateError::PlatformError(format!(
                        "Failed to copy portal screenshot output from {} to {}: {}",
                        source_path.display(),
                        output_path.display(),
                        err
                    ))
                })?;
                // Portal staging files under /run/user/$UID/doc/.../ persist under some
                // backends; remove after the copy. Some implementations may not own the
                // file (or may have already cleaned it); swallow failures.
                let _ = fs::remove_file(&source_path);
                Ok(PortalCaptureStatus::Captured)
            }
            1 => Ok(PortalCaptureStatus::Cancelled),
            other => Err(SlateError::PlatformError(format!(
                "Portal screenshot request failed with response code {}.",
                other
            ))),
        }
    }

    #[cfg(test)]
    mod tests {
        use super::{file_uri_to_path, owned_value_to_string, owned_value_to_u32};
        use zbus::zvariant::OwnedValue;

        #[test]
        fn test_owned_value_to_u32_reads_portal_color_scheme_values() {
            assert_eq!(owned_value_to_u32(&OwnedValue::from(1u32)).unwrap(), 1);
            assert_eq!(owned_value_to_u32(&OwnedValue::from(2u32)).unwrap(), 2);
        }

        #[test]
        fn test_owned_value_to_string_reads_portal_uri_values() {
            let uri = "file:///tmp/slate-share.png".to_string();
            assert_eq!(
                owned_value_to_string(&OwnedValue::from(uri.clone())).unwrap(),
                uri
            );
        }

        #[test]
        fn test_file_uri_to_path_rejects_non_file_uris() {
            assert!(file_uri_to_path("https://example.com/test.png").is_err());
        }
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
