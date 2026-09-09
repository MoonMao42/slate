//! Portal wire contracts shared by Linux production and isolated protocol tests.
use std::collections::HashMap;
use zbus::{
    proxy,
    zvariant::{OwnedObjectPath, OwnedValue},
};

#[proxy(
    interface = "org.freedesktop.portal.Settings",
    default_service = "org.freedesktop.portal.Desktop",
    default_path = "/org/freedesktop/portal/desktop"
)]
pub(super) trait PortalSettings {
    // Portal properties are lowercase; zbus's default mapping would use Version.
    #[zbus(property, name = "version")]
    fn version(&self) -> zbus::Result<u32>;

    #[zbus(name = "ReadOne")]
    fn read_one(&self, namespace: &str, key: &str) -> zbus::Result<OwnedValue>;

    #[zbus(name = "Read")]
    fn read_legacy(&self, namespace: &str, key: &str) -> zbus::Result<OwnedValue>;

    #[zbus(signal)]
    fn setting_changed(&self, namespace: &str, key: &str, value: OwnedValue) -> zbus::Result<()>;
}

#[proxy(
    interface = "org.freedesktop.portal.Screenshot",
    default_service = "org.freedesktop.portal.Desktop",
    default_path = "/org/freedesktop/portal/desktop"
)]
pub(super) trait PortalScreenshot {
    #[zbus(property, name = "version")]
    fn version(&self) -> zbus::Result<u32>;

    fn screenshot(
        &self,
        parent_window: &str,
        options: HashMap<&str, OwnedValue>,
    ) -> zbus::Result<OwnedObjectPath>;
}
