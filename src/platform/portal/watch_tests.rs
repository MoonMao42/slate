use super::*;
use crate::platform::portal::test_bus::{Bus, Mode, OWNER};
use futures_lite::io::AsyncReadExt;
use std::{
    io::Read, net::Shutdown, os::unix::net::UnixStream, sync::mpsc, thread::JoinHandle,
    time::Instant,
};

struct Watch {
    cancel: UnixStream,
    worker: Option<JoinHandle<()>>,
    ready: mpsc::Receiver<()>,
    values: mpsc::Receiver<u32>,
    result: mpsc::Receiver<Result<()>>,
}

impl Watch {
    fn new<F>(connect: impl FnOnce() -> F + Send + 'static) -> Self
    where
        F: Future<Output = zbus::Result<Connection>>,
    {
        let (cancel, read) = UnixStream::pair().unwrap();
        let mut read = async_io::Async::new(read).unwrap();
        let (send_ready, ready) = mpsc::channel();
        let (send_value, values) = mpsc::channel();
        let (send_result, result) = mpsc::channel();
        let worker = std::thread::spawn(move || {
            let result = async_io::block_on(run(
                connect(),
                async {
                    let mut byte = [0];
                    let _ = read.read(&mut byte).await;
                },
                || {
                    send_ready.send(()).unwrap();
                    Ok(())
                },
                |value| {
                    send_value.send(value).unwrap();
                    Ok(())
                },
            ));
            let _ = send_result.send(result);
        });
        Self {
            cancel,
            worker: Some(worker),
            ready,
            values,
            result,
        }
    }
    fn ready(&self) {
        self.ready.recv_timeout(Duration::from_secs(3)).unwrap();
    }
    fn result(&self) -> Result<()> {
        self.result.recv_timeout(Duration::from_secs(4)).unwrap()
    }
    fn stop(&self) {
        self.cancel.shutdown(Shutdown::Both).unwrap();
    }
}
impl Drop for Watch {
    fn drop(&mut self) {
        let _ = self.cancel.shutdown(Shutdown::Both);
        if let Some(worker) = self.worker.take() {
            worker.join().unwrap();
        }
    }
}

fn changed(sender: &str, namespace: &str, key: &str, value: Value<'_>) -> Message {
    Message::signal(PATH, INTERFACE, "SettingChanged")
        .unwrap()
        .sender(sender)
        .unwrap()
        .build(&(namespace, key, value))
        .unwrap()
}

#[test]
fn portal_watch_valid_signals_ignore_unrelated_and_spoofed_senders() {
    let mut bus = Bus::new(Mode::Normal);
    let connection = bus.take_client();
    let watch = Watch::new(move || std::future::ready(Ok(connection)));
    watch.ready();
    assert_eq!(bus.matches.load(std::sync::atomic::Ordering::SeqCst), 2);
    for message in [
        changed(
            OWNER,
            "unrelated.namespace",
            COLOR_SCHEME_KEY,
            Value::from("PRIVATE"),
        ),
        changed(OWNER, APPEARANCE_NAMESPACE, "unrelated-key", Value::U32(1)),
        changed(
            ":1.99",
            APPEARANCE_NAMESPACE,
            COLOR_SCHEME_KEY,
            Value::U32(1),
        ),
    ] {
        bus.send(&message);
    }
    bus.owner_change(":1.99", ""); // Raw unicast must not impersonate the daemon.
    for value in [0, 1, 2, 99] {
        bus.send(&changed(
            OWNER,
            APPEARANCE_NAMESPACE,
            COLOR_SCHEME_KEY,
            Value::U32(value),
        ));
        assert_eq!(
            watch.values.recv_timeout(Duration::from_secs(2)).unwrap(),
            value
        );
    }
    assert!(watch.values.try_recv().is_err());
    watch.stop();
    watch.result().unwrap();
    drop(watch);
    bus.assert_client_closed();
}

#[test]
fn portal_watch_invalid_relevant_values_fail_without_private_contents() {
    for message in [
        changed(
            OWNER,
            APPEARANCE_NAMESPACE,
            COLOR_SCHEME_KEY,
            Value::from("PRIVATE_VALUE"),
        ),
        changed(
            OWNER,
            APPEARANCE_NAMESPACE,
            COLOR_SCHEME_KEY,
            Value::Value(Box::new(Value::U32(1))),
        ),
        Message::signal(PATH, INTERFACE, "SettingChanged")
            .unwrap()
            .sender(OWNER)
            .unwrap()
            .build(&("PRIVATE malformed body",))
            .unwrap(),
    ] {
        let mut bus = Bus::new(Mode::Normal);
        let connection = bus.take_client();
        let watch = Watch::new(move || std::future::ready(Ok(connection)));
        watch.ready();
        bus.send(&message);
        let error = watch.result().unwrap_err().to_string();
        assert!(error.contains("invalid"), "{error}");
        assert!(!error.contains("PRIVATE"), "{error}");
        assert!(watch.values.try_recv().is_err());
        drop(watch);
        bus.assert_client_closed();
    }
}

#[test]
fn portal_watch_owner_loss_or_replacement_ends_the_subscription() {
    for new in ["", ":1.99"] {
        let mut bus = Bus::new(Mode::Normal);
        let connection = bus.take_client();
        let watch = Watch::new(move || std::future::ready(Ok(connection)));
        watch.ready();
        bus.owner_change("org.freedesktop.DBus", new);
        let error = watch.result().unwrap_err().to_string();
        assert!(
            error.contains("service owner disappeared or changed"),
            "{error}"
        );
        drop(watch);
        bus.assert_client_closed();
    }
}

#[test]
fn portal_watch_bus_disconnect_is_not_a_successful_idle_watcher() {
    let mut bus = Bus::new(Mode::Normal);
    let connection = bus.take_client();
    let watch = Watch::new(move || std::future::ready(Ok(connection)));
    watch.ready();
    super::super::query_deadline::run(async {
        bus.server.clone().close().await.unwrap();
        Ok(())
    })
    .unwrap();
    assert!(watch.result().is_err());
}

#[test]
fn portal_watch_startup_errors_never_report_ready_and_close_the_transport() {
    for (mode, expected) in [
        (Mode::Denied, "access denied"),
        (Mode::MissingOwner, "service owner"),
        (Mode::StalledSettings, "startup timed out"),
        (
            Mode::OwnerLostDuringSetup,
            "service owner disappeared or changed",
        ),
    ] {
        let mut bus = Bus::new(mode);
        let connection = bus.take_client();
        let watch = Watch::new(move || std::future::ready(Ok(connection)));
        let error = watch.result().unwrap_err().to_string();
        assert!(error.contains(expected), "{error}");
        assert!(!error.contains("PRIVATE"));
        assert!(watch.ready.try_recv().is_err());
        drop(watch);
        bus.assert_client_closed();
    }
}

#[test]
fn portal_watch_idle_has_no_query_deadline_and_cancels_without_a_signal() {
    let mut bus = Bus::new(Mode::Normal);
    let connection = bus.take_client();
    let watch = Watch::new(move || std::future::ready(Ok(connection)));
    watch.ready();
    assert!(matches!(
        watch
            .result
            .recv_timeout(QUERY_TIMEOUT + Duration::from_millis(50)),
        Err(mpsc::RecvTimeoutError::Timeout)
    ));
    let started = Instant::now();
    watch.stop();
    watch.result().unwrap();
    drop(watch);
    assert!(started.elapsed() < Duration::from_secs(2));
    bus.assert_client_closed();
}

#[test]
fn portal_watch_cancel_interrupts_an_unfinished_authentication() {
    let (mut peer, client) = UnixStream::pair().unwrap();
    peer.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
    let watch = Watch::new(move || zbus::connection::Builder::unix_stream(client).build());
    let mut bytes = [0; 256];
    assert!(
        peer.read(&mut bytes).unwrap() > 0,
        "client must actually start authentication"
    );
    watch.stop();
    watch.result().unwrap();
    drop(watch);
    while peer.read(&mut bytes).unwrap() != 0 {}
}
