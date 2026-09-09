//! Exercise actual method/signal messages over the private bus socket fixture.
//! No session bus, real screenshot helper, desktop UI or user configuration.
use super::*;
use crate::platform::{
    portal::test_bus::{Bus, Mode, OWNER, PATH},
    share::image_file::FIXTURE_PNG,
};
use std::{
    cell::Cell,
    fs,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    },
    time::Instant,
};

#[derive(Clone, Copy)]
enum Reply {
    Image,
    Cancel,
    Failed,
    MissingUri,
    WrongUri,
    NestedUri,
    Malformed,
    Large,
    Noisy,
    DeniedAfterImage,
    Hold,
    Flood,
}

#[derive(Default)]
struct Calls {
    version: AtomicUsize,
    screenshots: AtomicUsize,
    handles: Mutex<Vec<String>>,
    tokens: Mutex<Vec<String>>,
    interactive: Mutex<Vec<Option<bool>>>,
}

struct Screenshot {
    reply: Reply,
    uri: String,
    version: u32,
    legacy: bool,
    version_delay: Duration,
    calls: Arc<Calls>,
    rules: Arc<Mutex<Vec<String>>>,
}

fn response(path: &str, sender: &str, status: u32, values: HashMap<&str, Value<'_>>) -> Message {
    Message::signal(path, RESPONSE_INTERFACE, "Response")
        .unwrap()
        .sender(sender)
        .unwrap()
        .build(&(status, values))
        .unwrap()
}

fn image_response(path: &str, uri: &str) -> Message {
    response(path, OWNER, 0, HashMap::from([("uri", Value::from(uri))]))
}

#[zbus::interface(name = "org.freedesktop.portal.Screenshot")]
impl Screenshot {
    #[zbus(property, name = "version")]
    async fn version(&self) -> u32 {
        self.calls.version.fetch_add(1, Ordering::SeqCst);
        if !self.version_delay.is_zero() {
            async_io::Timer::after(self.version_delay).await;
        }
        self.version
    }

    async fn screenshot(
        &self,
        parent_window: &str,
        options: HashMap<String, OwnedValue>,
        #[zbus(connection)] connection: &Connection,
    ) -> zbus::fdo::Result<OwnedObjectPath> {
        assert_eq!(parent_window, "");
        // Deterministic ordering assertion: registration must have completed
        // BEFORE this method runs, not after it returns a path.
        let rules = self.rules.lock().unwrap().clone();
        assert!(rules
            .iter()
            .any(|rule| rule.contains("Response") && rule.contains(OWNER)));
        let token = <&str>::try_from(options.get("handle_token").unwrap()).unwrap();
        assert!(token.starts_with("slate_shot_"));
        assert_eq!(token.len(), "slate_shot_".len() + 64);
        assert!(token["slate_shot_".len()..]
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit()));
        self.calls.tokens.lock().unwrap().push(token.to_owned());
        self.calls.interactive.lock().unwrap().push(
            options
                .get("interactive")
                .map(|value| bool::try_from(value).unwrap()),
        );
        let handle = if self.legacy {
            "/org/freedesktop/portal/desktop/request/legacy_handle".to_owned()
        } else {
            format!("/org/freedesktop/portal/desktop/request/1_42/{token}")
        };
        self.calls.handles.lock().unwrap().push(handle.clone());
        self.calls.screenshots.fetch_add(1, Ordering::SeqCst);
        let message = match self.reply {
            Reply::Image | Reply::DeniedAfterImage => image_response(&handle, &self.uri),
            Reply::Cancel => response(&handle, OWNER, 1, HashMap::new()),
            Reply::Failed => response(&handle, OWNER, 2, HashMap::new()),
            Reply::MissingUri => response(&handle, OWNER, 0, HashMap::new()),
            Reply::WrongUri => response(&handle, OWNER, 0, HashMap::from([("uri", Value::U32(7))])),
            Reply::NestedUri => response(
                &handle,
                OWNER,
                0,
                HashMap::from([(
                    "uri",
                    Value::Value(Box::new(Value::from(self.uri.as_str()))),
                )]),
            ),
            Reply::Malformed => Message::signal(handle.as_str(), RESPONSE_INTERFACE, "Response")
                .unwrap()
                .sender(OWNER)
                .unwrap()
                .build(&("PRIVATE invalid body",))
                .unwrap(),
            Reply::Large => image_response(&handle, &"PRIVATE".repeat(MAX_RESPONSE_BYTES)),
            Reply::Noisy => {
                let wrong_owner = Message::signal(
                    "/org/freedesktop/DBus",
                    "org.freedesktop.DBus",
                    "NameOwnerChanged",
                )
                .unwrap()
                .sender(":1.99")
                .unwrap()
                .build(&(SERVICE, OWNER, ""))
                .unwrap();
                for message in [
                    response(&handle, ":1.99", 1, HashMap::new()),
                    response("/different_request", OWNER, 1, HashMap::new()),
                    Message::signal(handle.as_str(), "org.example.Wrong", "Response")
                        .unwrap()
                        .sender(OWNER)
                        .unwrap()
                        .build(&("PRIVATE",))
                        .unwrap(),
                    Message::signal(handle.as_str(), RESPONSE_INTERFACE, "WrongMember")
                        .unwrap()
                        .sender(OWNER)
                        .unwrap()
                        .build(&("PRIVATE",))
                        .unwrap(),
                    wrong_owner,
                ] {
                    connection.send(&message).await.unwrap();
                }
                image_response(&handle, &self.uri)
            }
            Reply::Hold => return Ok(handle.try_into().unwrap()),
            Reply::Flood => {
                for index in 0..MAX_PENDING + 8 {
                    let message =
                        response(&format!("/unrelated_{index}"), OWNER, 1, HashMap::new());
                    if connection.send(&message).await.is_err() {
                        break;
                    }
                }
                // Keep the method unresolved while the client must drain its
                // bounded early-response buffer and reject overflow.
                async_io::Timer::after(Duration::from_secs(1)).await;
                return Ok(handle.try_into().unwrap());
            }
        };
        connection.send(&message).await.unwrap();
        // Both modern and legacy fixtures emit Response BEFORE returning.
        if matches!(self.reply, Reply::DeniedAfterImage) {
            Err(zbus::fdo::Error::AccessDenied(
                "PRIVATE rejected screenshot".into(),
            ))
        } else {
            Ok(handle.try_into().unwrap())
        }
    }
}

fn bounded<T>(work: impl Future<Output = Result<T>>) -> Result<T> {
    async_io::block_on(future::or(work, async {
        async_io::Timer::after(Duration::from_secs(4)).await;
        panic!("isolated screenshot fixture did not finish");
    }))
}

async fn request(
    connect: impl Future<Output = zbus::Result<Connection>>,
    timeout: Duration,
) -> Result<Response> {
    request_with(connect, timeout, Ok).await
}

fn install(
    bus: &Bus,
    calls: &Arc<Calls>,
    reply: Reply,
    uri: &str,
    version: u32,
    legacy: bool,
    version_delay: Duration,
) {
    bounded(async {
        bus.server
            .object_server()
            .at(
                PATH,
                Screenshot {
                    reply,
                    uri: uri.to_owned(),
                    version,
                    legacy,
                    version_delay,
                    calls: calls.clone(),
                    rules: bus.rules.clone(),
                },
            )
            .await
            .unwrap();
        Ok(())
    })
    .unwrap();
}

struct Files {
    _root: tempfile::TempDir,
    source: std::path::PathBuf,
    output: std::path::PathBuf,
    uri: String,
}
impl Files {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("PRIVATE source %.png");
        let output = root.path().join("out/capture.png");
        fs::write(&source, FIXTURE_PNG).unwrap();
        let uri = url::Url::from_file_path(&source).unwrap().to_string();
        Self {
            _root: root,
            source,
            output,
            uri,
        }
    }
    fn intact(&self) {
        assert_eq!(fs::read(&self.source).unwrap(), FIXTURE_PNG);
    }
}

#[test]
fn portal_screenshot_early_modern_and_legacy_responses_save_borrowed_images() {
    let calls = Arc::new(Calls::default());
    for (version, legacy) in [(1, false), (2, false), (1, true), (3, true)] {
        let files = Files::new();
        let mut bus = Bus::new(Mode::Normal);
        install(
            &bus,
            &calls,
            Reply::Image,
            &files.uri,
            version,
            legacy,
            Duration::ZERO,
        );
        let connection = bus.take_client();
        assert_eq!(
            bounded(capture_with(
                std::future::ready(Ok(connection)),
                &files.output,
                QUERY_TIMEOUT
            ))
            .unwrap(),
            PortalCaptureStatus::Captured
        );
        files.intact();
        assert_eq!(fs::read(&files.output).unwrap(), FIXTURE_PNG);
        bus.assert_client_closed();
    }
    assert_eq!(
        *calls.interactive.lock().unwrap(),
        [None, Some(true), None, Some(true)]
    );
    let tokens = calls.tokens.lock().unwrap();
    assert_eq!(
        tokens
            .iter()
            .collect::<std::collections::HashSet<_>>()
            .len(),
        4
    );
    assert_eq!(calls.version.load(Ordering::SeqCst), 4);
}

#[test]
fn portal_screenshot_filters_sender_path_interface_member_and_forged_owner_loss() {
    let files = Files::new();
    let mut bus = Bus::new(Mode::Normal);
    install(
        &bus,
        &Arc::default(),
        Reply::Noisy,
        &files.uri,
        2,
        false,
        Duration::ZERO,
    );
    let connection = bus.take_client();
    assert_eq!(
        bounded(capture_with(
            std::future::ready(Ok(connection)),
            &files.output,
            QUERY_TIMEOUT
        ))
        .unwrap(),
        PortalCaptureStatus::Captured
    );
    files.intact();
    assert_eq!(fs::read(&files.output).unwrap(), FIXTURE_PNG);
    bus.assert_client_closed();
}

#[test]
fn portal_screenshot_cancel_and_invalid_responses_never_publish_an_image() {
    for reply in [
        Reply::Cancel,
        Reply::Failed,
        Reply::MissingUri,
        Reply::WrongUri,
        Reply::NestedUri,
        Reply::Malformed,
        Reply::Large,
        Reply::DeniedAfterImage,
    ] {
        let files = Files::new();
        let mut bus = Bus::new(Mode::Normal);
        install(
            &bus,
            &Arc::default(),
            reply,
            &files.uri,
            2,
            false,
            Duration::ZERO,
        );
        let connection = bus.take_client();
        let result = bounded(capture_with(
            std::future::ready(Ok(connection)),
            &files.output,
            QUERY_TIMEOUT,
        ));
        if matches!(reply, Reply::Cancel) {
            assert_eq!(result.unwrap(), PortalCaptureStatus::Cancelled);
        } else {
            let error = result.unwrap_err().to_string();
            assert!(!error.contains("PRIVATE"), "{error}");
            if matches!(reply, Reply::DeniedAfterImage) {
                assert!(error.contains("access denied"), "{error}");
            }
        }
        files.intact();
        assert!(!files.output.parent().unwrap().exists());
        bus.assert_client_closed();
    }
}

async fn wait_called(calls: &Calls) -> String {
    while calls.screenshots.load(Ordering::SeqCst) == 0 {
        async_io::Timer::after(Duration::from_millis(5)).await;
    }
    calls.handles.lock().unwrap().last().unwrap().clone()
}

#[test]
fn portal_screenshot_owner_loss_replacement_and_disconnect_end_interaction() {
    for new_owner in [Some(""), Some(":1.99"), None] {
        let mut bus = Bus::new(Mode::Normal);
        let calls = Arc::default();
        install(
            &bus,
            &calls,
            Reply::Hold,
            "PRIVATE",
            2,
            false,
            Duration::ZERO,
        );
        let connection = bus.take_client();
        let error = bounded(async {
            let (result, ()) = future::zip(
                request(std::future::ready(Ok(connection)), QUERY_TIMEOUT),
                async {
                    wait_called(&calls).await;
                    if let Some(new) = new_owner {
                        bus.server
                            .send(
                                &Message::signal(
                                    "/org/freedesktop/DBus",
                                    "org.freedesktop.DBus",
                                    "NameOwnerChanged",
                                )
                                .unwrap()
                                .sender("org.freedesktop.DBus")
                                .unwrap()
                                .build(&(SERVICE, OWNER, new))
                                .unwrap(),
                            )
                            .await
                            .unwrap();
                    } else {
                        bus.server.clone().close().await.unwrap();
                    }
                },
            )
            .await;
            result
        })
        .unwrap_err()
        .to_string();
        assert!(!error.contains("PRIVATE"), "{error}");
        if new_owner.is_some() {
            assert!(error.contains("owner disappeared or changed"), "{error}");
        }
        bus.assert_client_closed();
    }
}

#[test]
fn portal_screenshot_setup_errors_and_deadlines_do_not_invoke_capture() {
    for mode in [
        Mode::Denied,
        Mode::StalledResponse,
        Mode::MissingOwner,
        Mode::OwnerLostDuringScreenshotSetup,
    ] {
        let mut bus = Bus::new(mode);
        let calls = Arc::default();
        install(
            &bus,
            &calls,
            Reply::Image,
            "PRIVATE",
            2,
            false,
            Duration::ZERO,
        );
        let connection = bus.take_client();
        let error = bounded(request(
            std::future::ready(Ok(connection)),
            Duration::from_millis(200),
        ))
        .unwrap_err()
        .to_string();
        assert!(!error.contains("PRIVATE"), "{error}");
        assert_eq!(calls.screenshots.load(Ordering::SeqCst), 0);
        bus.assert_client_closed();
    }
    let error = bounded(request(std::future::pending(), Duration::from_millis(50)))
        .unwrap_err()
        .to_string();
    assert!(error.contains("timed out"), "{error}");
}

#[test]
fn portal_screenshot_user_interaction_outlives_the_setup_deadline() {
    let files = Files::new();
    let mut bus = Bus::new(Mode::Normal);
    let calls = Arc::default();
    install(
        &bus,
        &calls,
        Reply::Hold,
        &files.uri,
        2,
        false,
        Duration::ZERO,
    );
    let connection = bus.take_client();
    let started = Instant::now();
    let status = bounded(async {
        let (result, ()) = future::zip(
            capture_with(
                std::future::ready(Ok(connection)),
                &files.output,
                Duration::from_millis(300),
            ),
            async {
                let path = wait_called(&calls).await;
                async_io::Timer::after(Duration::from_millis(500)).await;
                bus.server
                    .send(&image_response(&path, &files.uri))
                    .await
                    .unwrap();
            },
        )
        .await;
        result
    })
    .unwrap();
    assert_eq!(status, PortalCaptureStatus::Captured);
    assert!(started.elapsed() >= Duration::from_millis(500));
    files.intact();
    bus.assert_client_closed();
}

#[test]
fn portal_screenshot_early_response_flood_fails_with_bounded_buffering() {
    let mut bus = Bus::new(Mode::Normal);
    install(
        &bus,
        &Arc::default(),
        Reply::Flood,
        "PRIVATE",
        2,
        false,
        Duration::ZERO,
    );
    let connection = bus.take_client();
    let error = bounded(request(std::future::ready(Ok(connection)), QUERY_TIMEOUT))
        .unwrap_err()
        .to_string();
    assert!(error.contains("buffering limit"), "{error}");
    assert!(!error.contains("PRIVATE"));
    bus.assert_client_closed();
}

#[test]
fn portal_screenshot_version_queries_are_bounded_and_do_not_request_images() {
    for delay in [Duration::ZERO, Duration::from_secs(1)] {
        let mut bus = Bus::new(Mode::Normal);
        let calls = Arc::default();
        install(&bus, &calls, Reply::Image, "PRIVATE", 1, false, delay);
        let connection = bus.take_client();
        let result = bounded(version_query(
            std::future::ready(Ok(connection)),
            Duration::from_millis(200),
        ));
        if delay.is_zero() {
            assert_eq!(result.unwrap(), 1);
        } else {
            assert!(result.unwrap_err().to_string().contains("timed out"));
        }
        assert_eq!(calls.version.load(Ordering::SeqCst), 1);
        assert_eq!(calls.screenshots.load(Ordering::SeqCst), 0);
        bus.assert_client_closed();
    }
}

#[test]
fn portal_screenshot_activates_a_missing_service_before_response_subscription() {
    let mut bus = Bus::new(Mode::Activatable);
    install(
        &bus,
        &Arc::default(),
        Reply::Cancel,
        "PRIVATE",
        1,
        false,
        Duration::ZERO,
    );
    let connection = bus.take_client();
    assert_eq!(
        bounded(request(std::future::ready(Ok(connection)), QUERY_TIMEOUT)).unwrap(),
        Response::Cancelled
    );
    assert_eq!(bus.starts.load(Ordering::SeqCst), 1);
    bus.assert_client_closed();
}

#[test]
fn portal_screenshot_keeps_transport_live_until_borrowed_result_is_consumed() {
    let mut bus = Bus::new(Mode::Normal);
    install(
        &bus,
        &Arc::default(),
        Reply::Image,
        "file:///PRIVATE",
        2,
        false,
        Duration::ZERO,
    );
    let connection = bus.take_client();
    let probe = zbus::blocking::Connection::from(connection.clone());
    let error = bounded(request_with(
        std::future::ready(Ok(connection)),
        QUERY_TIMEOUT,
        |response| {
            assert_eq!(response, Response::Uri("file:///PRIVATE".into()));
            // This real round trip must still work inside the consumer, even if it
            // fails to read/save the borrowed file. Cleanup happens afterwards.
            let reply = probe
                .call_method(
                    Some("org.freedesktop.DBus"),
                    "/org/freedesktop/DBus",
                    Some("org.freedesktop.DBus"),
                    "GetNameOwner",
                    &(SERVICE,),
                )
                .unwrap();
            assert_eq!(reply.body().deserialize::<&str>().unwrap(), OWNER);
            Err::<(), _>(error("fixture file handoff failed"))
        },
    ))
    .unwrap_err()
    .to_string();
    assert!(error.contains("file handoff failed"));
    assert!(!error.contains("PRIVATE"));
    // Explicit transport closure must also close the test's remaining clone.
    bus.assert_client_closed();
}

#[test]
fn portal_screenshot_existing_output_is_rejected_before_connecting() {
    let files = Files::new();
    fs::create_dir_all(files.output.parent().unwrap()).unwrap();
    let connected = Cell::new(false);
    for linked in [false, true] {
        if linked {
            std::os::unix::fs::symlink(&files.source, &files.output).unwrap();
        } else {
            fs::write(&files.output, "original").unwrap();
        }
        let error = bounded(capture_with(
            async {
                connected.set(true);
                panic!("existing output must not connect");
            },
            &files.output,
            QUERY_TIMEOUT,
        ))
        .unwrap_err()
        .to_string();
        assert!(error.contains("already exists"));
        assert!(!connected.get());
        if linked {
            assert!(fs::symlink_metadata(&files.output)
                .unwrap()
                .file_type()
                .is_symlink());
        } else {
            assert_eq!(fs::read(&files.output).unwrap(), b"original");
        }
        fs::remove_file(&files.output).unwrap();
        files.intact();
    }
}
