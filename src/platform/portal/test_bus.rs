//! Minimal isolated message-bus fixture over an unnamed socket pair. The client
//! performs Hello/AddMatch/GetNameOwner against this fixture, never a session bus.
use super::query_deadline;
use futures_lite::StreamExt;
use std::{
    os::unix::net::UnixStream,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    },
};
use zbus::{Connection, Message, MessageStream};

pub(crate) const SERVICE: &str = "org.freedesktop.portal.Desktop";
pub(crate) const OWNER: &str = ":1.7";
pub(crate) const PATH: &str = "/org/freedesktop/portal/desktop";
pub(crate) const INTERFACE: &str = "org.freedesktop.portal.Settings";

#[derive(Clone, Copy)]
pub(crate) enum Mode {
    Normal,
    Denied,
    MissingOwner,
    StalledSettings,
    OwnerLostDuringSetup,
    Activatable,
    StalledResponse,
    OwnerLostDuringScreenshotSetup,
}

struct Daemon {
    mode: Mode,
    matches: Arc<AtomicUsize>,
    rules: Arc<Mutex<Vec<String>>>,
    starts: Arc<AtomicUsize>,
}

#[zbus::interface(name = "org.freedesktop.DBus")]
impl Daemon {
    fn hello(&self) -> &str {
        ":1.42"
    }
    fn get_name_owner(&self, name: &str) -> zbus::fdo::Result<&str> {
        assert_eq!(name, SERVICE);
        if matches!(self.mode, Mode::MissingOwner)
            || (matches!(self.mode, Mode::Activatable) && self.starts.load(Ordering::SeqCst) == 0)
        {
            Err(zbus::fdo::Error::NameHasNoOwner(
                "PRIVATE owner detail".into(),
            ))
        } else {
            Ok(OWNER)
        }
    }
    fn start_service_by_name(&self, name: &str, flags: u32) -> zbus::fdo::Result<u32> {
        assert_eq!((name, flags), (SERVICE, 0));
        self.starts.fetch_add(1, Ordering::SeqCst);
        if matches!(self.mode, Mode::Activatable) {
            Ok(1)
        } else {
            Err(zbus::fdo::Error::ServiceUnknown(
                "PRIVATE activation detail".into(),
            ))
        }
    }
    async fn add_match(
        &self,
        rule: &str,
        #[zbus(connection)] connection: &Connection,
    ) -> zbus::fdo::Result<()> {
        self.matches.fetch_add(1, Ordering::SeqCst);
        self.rules.lock().unwrap().push(rule.to_owned());
        if matches!(self.mode, Mode::Denied) {
            return Err(zbus::fdo::Error::AccessDenied(
                "PRIVATE subscription detail".into(),
            ));
        }
        if matches!(self.mode, Mode::StalledSettings) && rule.contains("SettingChanged") {
            async_io::Timer::after(std::time::Duration::from_secs(10)).await;
        }
        if matches!(self.mode, Mode::StalledResponse) && rule.contains("Response") {
            async_io::Timer::after(std::time::Duration::from_secs(10)).await;
        }
        if (matches!(self.mode, Mode::OwnerLostDuringSetup) && rule.contains("SettingChanged"))
            || (matches!(self.mode, Mode::OwnerLostDuringScreenshotSetup)
                && rule.contains("Response"))
        {
            let lost = Message::signal(
                "/org/freedesktop/DBus",
                "org.freedesktop.DBus",
                "NameOwnerChanged",
            )
            .unwrap()
            .sender("org.freedesktop.DBus")
            .unwrap()
            .build(&(SERVICE, OWNER, ""))
            .unwrap();
            connection.send(&lost).await.unwrap();
        }
        Ok(())
    }
    fn remove_match(&self, _rule: &str) {}
}

pub(crate) struct Bus {
    pub server: Connection,
    client: Option<Connection>,
    closed: MessageStream,
    pub matches: Arc<AtomicUsize>,
    pub rules: Arc<Mutex<Vec<String>>>,
    pub starts: Arc<AtomicUsize>,
}

impl Bus {
    pub fn new(mode: Mode) -> Self {
        query_deadline::run(async {
            let (server, client) = UnixStream::pair().unwrap();
            let matches = Arc::new(AtomicUsize::new(0));
            let rules = Arc::new(Mutex::new(Vec::new()));
            let starts = Arc::new(AtomicUsize::new(0));
            let server = zbus::connection::Builder::unix_stream(server)
                .server(zbus::Guid::generate())
                .unwrap()
                .p2p()
                .serve_at(
                    "/org/freedesktop/DBus",
                    Daemon {
                        mode,
                        matches: matches.clone(),
                        rules: rules.clone(),
                        starts: starts.clone(),
                    },
                )
                .unwrap()
                .build();
            // Unlike the Settings value fixture, this client acts as a bus
            // client, exercising real Hello/AddMatch request and reply messages.
            let client = zbus::connection::Builder::unix_stream(client).build();
            let (server, client) = futures_lite::future::zip(server, client).await;
            let (server, client) = (server.unwrap(), client.unwrap());
            assert!(client.is_bus());
            let closed = MessageStream::from(&server);
            Ok(Self {
                server,
                client: Some(client),
                closed,
                matches,
                rules,
                starts,
            })
        })
        .unwrap()
    }
    pub fn take_client(&mut self) -> Connection {
        self.client.take().unwrap()
    }
    pub fn send(&self, message: &Message) {
        query_deadline::run(async {
            self.server.send(message).await.unwrap();
            Ok(())
        })
        .unwrap();
    }
    pub fn owner_change(&self, sender: &str, new: &str) {
        self.send(
            &Message::signal(
                "/org/freedesktop/DBus",
                "org.freedesktop.DBus",
                "NameOwnerChanged",
            )
            .unwrap()
            .sender(sender)
            .unwrap()
            .build(&(SERVICE, OWNER, new))
            .unwrap(),
        );
    }
    pub fn assert_client_closed(&mut self) {
        query_deadline::run(async {
            while let Some(Ok(_)) = self.closed.next().await {}
            Ok(())
        })
        .expect("portal operation must close its owned transport, not just abandon its stream");
    }
}

impl Drop for Bus {
    fn drop(&mut self) {
        let server = self.server.clone();
        let client = self.client.take();
        let _ = query_deadline::run(async {
            if let Some(client) = client {
                let _ = client.close().await;
            }
            let _ = server.close().await;
            Ok(())
        });
    }
}
