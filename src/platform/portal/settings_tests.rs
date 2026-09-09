//! Real D-Bus messages over an unnamed private socket pair. No session bus,
//! desktop service, helper process, environment mutation or configuration writes.
use super::*;
use std::{
    os::unix::net::UnixStream,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

#[derive(Default)]
struct Calls {
    read_one: AtomicUsize,
    read: AtomicUsize,
    version: AtomicUsize,
}

#[derive(Clone, Copy)]
enum Reply {
    Number(u32),
    Nested(u32),
    TooNested,
    Text,
    UnknownMethod,
    UnknownObject,
    Denied,
    Failed,
}

impl Reply {
    fn value(self) -> zbus::fdo::Result<OwnedValue> {
        match self {
            Self::Number(value) => Ok(value.into()),
            Self::Nested(value) => Ok(Value::Value(Box::new(Value::U32(value)))
                .try_to_owned()
                .unwrap()),
            Self::TooNested => Ok(
                Value::Value(Box::new(Value::Value(Box::new(Value::U32(1)))))
                    .try_to_owned()
                    .unwrap(),
            ),
            Self::Text => Ok(Value::from("PRIVATE_RESPONSE_VALUE")
                .try_to_owned()
                .unwrap()),
            Self::UnknownMethod => Err(zbus::fdo::Error::UnknownMethod(
                "PRIVATE method missing".into(),
            )),
            Self::UnknownObject => Err(zbus::fdo::Error::UnknownObject(
                "PRIVATE object missing".into(),
            )),
            Self::Denied => Err(zbus::fdo::Error::AccessDenied(
                "PRIVATE permission detail".into(),
            )),
            Self::Failed => Err(zbus::fdo::Error::Failed("PRIVATE failure detail".into())),
        }
    }
}

struct Fixture {
    one: Reply,
    legacy: Reply,
    one_delay: Duration,
    legacy_delay: Duration,
    calls: Arc<Calls>,
}

#[zbus::interface(name = "org.freedesktop.portal.Settings")]
impl Fixture {
    async fn read_one(&self, namespace: &str, key: &str) -> zbus::fdo::Result<OwnedValue> {
        assert_eq!((namespace, key), (APPEARANCE_NAMESPACE, COLOR_SCHEME_KEY));
        self.calls.read_one.fetch_add(1, Ordering::SeqCst);
        if !self.one_delay.is_zero() {
            async_io::Timer::after(self.one_delay).await;
        }
        self.one.value()
    }
    async fn read(&self, namespace: &str, key: &str) -> zbus::fdo::Result<OwnedValue> {
        assert_eq!((namespace, key), (APPEARANCE_NAMESPACE, COLOR_SCHEME_KEY));
        self.calls.read.fetch_add(1, Ordering::SeqCst);
        if !self.legacy_delay.is_zero() {
            async_io::Timer::after(self.legacy_delay).await;
        }
        self.legacy.value()
    }
    #[zbus(property, name = "version")]
    fn version(&self) -> u32 {
        self.calls.version.fetch_add(1, Ordering::SeqCst);
        2
    }
}

// This interface really has no ReadOne method. Let zbus itself return the
// unknown-method wire error, rather than relying only on a mocked error value.
struct LegacyOnly {
    value: u32,
    calls: Arc<Calls>,
}

#[zbus::interface(name = "org.freedesktop.portal.Settings")]
impl LegacyOnly {
    fn read(&self, namespace: &str, key: &str) -> OwnedValue {
        assert_eq!((namespace, key), (APPEARANCE_NAMESPACE, COLOR_SCHEME_KEY));
        self.calls.read.fetch_add(1, Ordering::SeqCst);
        Reply::Nested(self.value).value().unwrap()
    }
    #[zbus(property, name = "version")]
    fn version(&self) -> u32 {
        self.calls.version.fetch_add(1, Ordering::SeqCst);
        1
    }
}

// Expose only the version property, never a screenshot operation.
struct ScreenshotVersion {
    version: u32,
    calls: Arc<Calls>,
}

#[zbus::interface(name = "org.freedesktop.portal.Screenshot")]
impl ScreenshotVersion {
    #[zbus(property, name = "version")]
    fn version(&self) -> u32 {
        self.calls.version.fetch_add(1, Ordering::SeqCst);
        self.version
    }
}

struct Peer {
    server: Connection,
    client: Connection,
}

impl Peer {
    fn new(interface: impl zbus::object_server::Interface) -> Self {
        query_deadline::run(async {
            let (server, client) = UnixStream::pair().unwrap();
            let server = zbus::connection::Builder::unix_stream(server)
                .server(zbus::Guid::generate())
                .unwrap()
                .p2p()
                .serve_at("/org/freedesktop/portal/desktop", interface)
                .unwrap()
                .build();
            let client = zbus::connection::Builder::unix_stream(client).p2p().build();
            let (server, client) = futures_lite::future::zip(server, client).await;
            Ok(Self {
                server: server.unwrap(),
                client: client.unwrap(),
            })
        })
        .expect("private D-Bus handshake must finish within its fixture deadline")
    }
    fn query(&self) -> Result<Option<u32>> {
        query_deadline::run(read_query(std::future::ready(Ok(self.client.clone()))))
    }

    fn assert_uppercase_version_missing(&self, interface: &str) {
        let missing = query_deadline::run(async {
            let reply = self
                .client
                .call_method(
                    Some("org.freedesktop.portal.Desktop"),
                    "/org/freedesktop/portal/desktop",
                    Some("org.freedesktop.DBus.Properties"),
                    "Get",
                    &(interface, "Version"),
                )
                .await;
            Ok(matches!(reply, Err(zbus::Error::MethodError(name, _, _))
                if name.as_str() == "org.freedesktop.DBus.Error.UnknownProperty"))
        })
        .unwrap();
        assert!(
            missing,
            "fixture must not accept the incorrect Version alias"
        );
    }
}

impl Drop for Peer {
    fn drop(&mut self) {
        let client = self.client.clone();
        let server = self.server.clone();
        // Close these owned transports even when an assertion unwinds.
        let _ = query_deadline::run(async {
            let _ = client.close().await;
            let _ = server.close().await;
            Ok(())
        });
    }
}

fn fixture(one: Reply, legacy: Reply, calls: &Arc<Calls>) -> Fixture {
    Fixture {
        one,
        legacy,
        one_delay: Duration::ZERO,
        legacy_delay: Duration::ZERO,
        calls: calls.clone(),
    }
}

fn assert_calls(calls: &Calls, one: usize, legacy: usize, version: usize) {
    assert_eq!(calls.read_one.load(Ordering::SeqCst), one);
    assert_eq!(calls.read.load(Ordering::SeqCst), legacy);
    assert_eq!(calls.version.load(Ordering::SeqCst), version);
}

#[test]
fn portal_settings_read_one_and_version_use_private_wire_messages() {
    for value in [0, 1, 2, 99] {
        let calls = Arc::new(Calls::default());
        let peer = Peer::new(fixture(Reply::Number(value), Reply::Denied, &calls));
        assert_eq!(peer.query().unwrap(), Some(value));
        assert_calls(&calls, 1, 0, 0); // No implicit version/GetAll or legacy query.
        assert_eq!(
            query_deadline::run(version_query(std::future::ready(Ok(peer.client.clone()))))
                .unwrap(),
            2
        );
        peer.assert_uppercase_version_missing("org.freedesktop.portal.Settings");
        assert_calls(&calls, 1, 0, 1);
    }
}

#[test]
fn portal_settings_legacy_without_read_one_decodes_the_extra_variant() {
    for value in [0, 1, 2, 99] {
        let calls = Arc::new(Calls::default());
        let peer = Peer::new(LegacyOnly {
            value,
            calls: calls.clone(),
        });
        assert_eq!(peer.query().unwrap(), Some(value));
        assert_calls(&calls, 0, 1, 0);
        assert_eq!(
            query_deadline::run(version_query(std::future::ready(Ok(peer.client.clone()))))
                .unwrap(),
            1
        );
        peer.assert_uppercase_version_missing("org.freedesktop.portal.Settings");
        assert_calls(&calls, 0, 1, 1);
    }
}

#[test]
fn portal_settings_related_screenshot_version_uses_lowercase_property() {
    use super::super::interfaces::PortalScreenshotProxy;

    for version in [1, 2, 3] {
        let calls = Arc::new(Calls::default());
        let peer = Peer::new(ScreenshotVersion {
            version,
            calls: calls.clone(),
        });
        let actual = query_deadline::run(async {
            let proxy = PortalScreenshotProxy::builder(&peer.client)
                .cache_properties(zbus::proxy::CacheProperties::No)
                .build()
                .await
                .unwrap();
            Ok(proxy.version().await.unwrap())
        })
        .unwrap();
        assert_eq!(actual, version);
        peer.assert_uppercase_version_missing("org.freedesktop.portal.Screenshot");
        assert_calls(&calls, 0, 0, 1);
    }
}

#[test]
fn portal_settings_denied_or_invalid_read_one_never_attempts_legacy() {
    for reply in [
        Reply::Denied,
        Reply::Failed,
        Reply::Text,
        Reply::Nested(1),
        Reply::UnknownObject,
    ] {
        let calls = Arc::new(Calls::default());
        let peer = Peer::new(fixture(reply, Reply::Nested(2), &calls));
        let result = peer.query();
        if matches!(reply, Reply::UnknownObject) {
            assert_eq!(result.unwrap(), None);
        } else {
            let error = result.unwrap_err().to_string();
            assert!(error.contains("ReadOne"), "{error}");
            assert!(!error.contains("PRIVATE"), "{error}");
            if matches!(reply, Reply::Denied) {
                assert!(error.contains("access denied"));
            }
        }
        assert_calls(&calls, 1, 0, 0);
    }
}

#[test]
fn portal_settings_legacy_does_not_guess_types_or_hide_request_failures() {
    for reply in [
        Reply::Number(1),
        Reply::Text,
        Reply::TooNested,
        Reply::Denied,
        Reply::UnknownMethod,
    ] {
        let calls = Arc::new(Calls::default());
        let peer = Peer::new(fixture(Reply::UnknownMethod, reply, &calls));
        let result = peer.query();
        if matches!(reply, Reply::UnknownMethod) {
            assert_eq!(result.unwrap(), None);
        } else {
            let error = result.unwrap_err().to_string();
            assert!(error.contains("Read fallback"), "{error}");
            assert!(!error.contains("PRIVATE"), "{error}");
        }
        assert_calls(&calls, 1, 1, 0);
    }
}

#[test]
fn portal_settings_legacy_fallback_shares_the_original_deadline() {
    let calls = Arc::new(Calls::default());
    let mut service = fixture(Reply::UnknownMethod, Reply::Nested(1), &calls);
    service.one_delay = Duration::from_millis(1200);
    service.legacy_delay = Duration::from_millis(1200);
    let peer = Peer::new(service);
    let started = Instant::now();
    let error = peer.query().unwrap_err().to_string();
    assert!(error.contains("timed out after 2000 ms"), "{error}");
    assert!(started.elapsed() < Duration::from_secs(5));
    assert_calls(&calls, 1, 1, 0);
}

#[test]
fn portal_settings_timeout_does_not_start_a_legacy_request() {
    let calls = Arc::new(Calls::default());
    let mut service = fixture(Reply::UnknownMethod, Reply::Nested(1), &calls);
    service.one_delay = Duration::from_secs(3);
    let peer = Peer::new(service);
    let error = peer.query().unwrap_err().to_string();
    assert!(error.contains("timed out"), "{error}");
    assert_calls(&calls, 1, 0, 0);
}

#[test]
fn portal_settings_connection_failures_keep_type_but_not_private_details() {
    for (error, expected) in [
        (
            zbus::Error::Address("PRIVATE address".into()),
            "invalid session bus address",
        ),
        (
            std::io::Error::from(std::io::ErrorKind::PermissionDenied).into(),
            "PermissionDenied",
        ),
    ] {
        let error = query_deadline::run(read_query(std::future::ready(Err(error))))
            .unwrap_err()
            .to_string();
        assert!(error.contains("connection"));
        assert!(error.contains(expected), "{error}");
        assert!(!error.contains("PRIVATE"));
    }
    assert_eq!(
        query_deadline::run(read_query(std::future::ready(Err(std::io::Error::from(
            std::io::ErrorKind::NotFound
        )
        .into()))))
        .unwrap(),
        None
    );
}
