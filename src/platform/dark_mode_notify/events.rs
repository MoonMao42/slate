use super::{error, Result, SlateEnv};
use std::process::Command;

mod command;
#[cfg(any(target_os = "linux", test))]
mod portal;
mod queue;
use command::{Format, NativeSource};

#[derive(Debug)]
pub(super) enum Event {
    Changed,
    Failed(String),
}

pub(super) struct Source {
    pub events: queue::Receiver,
    _native: Option<NativeSource>,
    #[cfg(any(target_os = "linux", test))]
    _portal: Option<portal::PortalSource>,
}

impl Source {
    pub fn new(env: &SlateEnv) -> Result<Self> {
        #[cfg(target_os = "macos")]
        {
            let mut command =
                Command::new(env.config_dir().join("managed/bin").join(super::HELPER));
            command.arg("--events").arg(std::process::id().to_string());
            Self::command(command, Format::Macos)
        }
        #[cfg(target_os = "linux")]
        {
            use crate::platform::desktop::DesktopAppearanceBackend;
            let _ = env;
            match crate::platform::desktop::detect_backend() {
                DesktopAppearanceBackend::XdgDesktopPortal => {
                    Self::portal(zbus::Connection::session)
                }
                DesktopAppearanceBackend::GnomeGsettings => {
                    let gsettings = crate::detection::command_in_actual_path("gsettings")
                        .or_else(|| crate::detection::command_path("gsettings"))
                        .ok_or_else(|| error("gsettings is unavailable"))?;
                    let mut command = Command::new(gsettings);
                    command.args(["monitor", "org.gnome.desktop.interface", "color-scheme"]);
                    command.env("LC_ALL", "C");
                    Self::command(command, Format::Gnome)
                }
                _ => Err(error("Desktop appearance watching is unavailable")),
            }
        }
        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        {
            let _ = env;
            Err(error("Desktop appearance watching is unavailable"))
        }
    }

    fn command(command: Command, format: Format) -> Result<Self> {
        let (sender, events) = queue::channel();
        let native = NativeSource::spawn(command, format, sender)?;
        Ok(Self {
            events,
            _native: Some(native),
            #[cfg(any(target_os = "linux", test))]
            _portal: None,
        })
    }

    #[cfg(any(target_os = "linux", test))]
    fn portal<F>(connect: impl FnOnce() -> F + Send + 'static) -> Result<Self>
    where
        F: std::future::Future<Output = zbus::Result<zbus::Connection>>,
    {
        let (sender, events) = queue::channel();
        let portal = portal::PortalSource::spawn(connect, sender)?;
        Ok(Self {
            events,
            _native: None,
            _portal: Some(portal),
        })
    }

    #[cfg(test)]
    pub fn fixture() -> (Self, queue::Sender) {
        let (sender, events) = queue::channel();
        (
            Self {
                events,
                _native: None,
                _portal: None,
            },
            sender,
        )
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn watcher_native_helper_only_emits_events_and_is_reaped_by_its_owner() {
        // A private copy of the compiled helper. No Slate configuration is
        // passed and event mode cannot execute a theme command.
        assert!(!super::super::EMBEDDED_WATCHER.is_empty());
        let td = tempfile::tempdir().unwrap();
        let path = td.path().join("appearance-helper");
        std::fs::write(&path, super::super::EMBEDDED_WATCHER).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        let mut command = Command::new(path);
        command.arg("--events").arg(std::process::id().to_string());
        let source = Source::command(command, Format::Macos).unwrap();
        assert!(matches!(
            source
                .events
                .recv_timeout(std::time::Duration::from_secs(4)),
            Ok(Event::Changed)
        ));
        // Source owns the exact Child and waits for it on Drop.
        drop(source);
    }
}

#[cfg(test)]
#[path = "events_tests.rs"]
mod event_tests;
