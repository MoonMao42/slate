//! Private shell event sources only: no compiled Swift helper, GNOME service,
//! desktop settings, real theme application or global environment mutation.
use super::*;
use std::{
    sync::mpsc::RecvTimeoutError,
    time::{Duration, Instant},
};

fn source(body: &str, format: Format) -> Source {
    let mut command = Command::new("/bin/sh");
    command.env_clear().args(["-c", body]);
    Source::command(command, format).unwrap()
}

fn changed(source: &Source) {
    assert!(matches!(
        source.events.recv_timeout(Duration::from_secs(3)),
        Ok(Event::Changed)
    ));
}

#[test]
fn watcher_event_queue_coalesces_floods_without_hiding_terminal_failure() {
    let (sender, receiver) = queue::channel();
    for _ in 0..100_000 {
        sender.send(Event::Changed).unwrap();
    }
    assert!(matches!(
        receiver.recv_timeout(Duration::ZERO),
        Ok(Event::Changed)
    ));
    assert!(matches!(
        receiver.recv_timeout(Duration::ZERO),
        Err(RecvTimeoutError::Timeout)
    ));
    sender.send(Event::Changed).unwrap();
    sender
        .send(Event::Failed("first source failure".into()))
        .unwrap();
    sender
        .send(Event::Failed("later source failure".into()))
        .unwrap();
    assert!(
        matches!(receiver.recv_timeout(Duration::ZERO), Ok(Event::Failed(message))
        if message == "first source failure")
    );
    drop(receiver);
    assert!(sender.send(Event::Changed).is_err());

    let (sender, receiver) = queue::channel();
    drop(sender);
    assert!(matches!(
        receiver.recv_timeout(Duration::ZERO),
        Err(RecvTimeoutError::Disconnected)
    ));
}

#[test]
fn watcher_event_native_parser_accepts_only_its_backend_complete_records() {
    for (format, invalid, valid) in [
        (Format::Macos, "printf \"color-scheme: 'prefer-dark'\\nPRIVATE dark error\\n\\377\\n\"; exec /bin/sleep 10", "printf 'dark\\nlight\\n'; exec /bin/sleep 10"),
        (Format::Gnome, "printf \"dark\\nPRIVATE color-scheme failed\\ncolor-scheme: 'PRIVATE prefer-dark'\\n\\377\\n\"; exec /bin/sleep 10", "printf \"color-scheme: 'prefer-dark'\\ncolor-scheme: 'prefer-light'\\ncolor-scheme: 'default'\\n\"; exec /bin/sleep 10"),
    ] {
        let invalid = source(invalid, format);
        assert!(matches!(invalid.events.recv_timeout(Duration::from_millis(100)), Err(RecvTimeoutError::Timeout)));
        drop(invalid);
        let valid = source(valid, format);
        changed(&valid);
        drop(valid);
    }
}

#[test]
fn watcher_event_reader_handles_split_records_but_never_emits_truncated_eof() {
    let split = source(
        "printf da; /bin/sleep 0.05; printf 'rk\\r\\n'; exec /bin/sleep 10",
        Format::Macos,
    );
    changed(&split);
    drop(split);
    let truncated = source("printf dark", Format::Macos);
    assert!(
        matches!(truncated.events.recv_timeout(Duration::from_secs(3)), Ok(Event::Failed(message))
        if message == "Appearance event source exited")
    );
}

#[test]
fn watcher_event_oversized_unterminated_record_fails_without_echoing_output() {
    let source = source("printf PRIVATE; i=0; while [ \"$i\" -lt 1100 ]; do printf x; i=$((i+1)); done; exec /bin/sleep 10", Format::Macos);
    let Event::Failed(message) = source.events.recv_timeout(Duration::from_secs(3)).unwrap() else {
        panic!("oversized output must fail, not emit an appearance change");
    };
    assert!(message.contains("exceeded 1024 bytes"), "{message}");
    assert!(!message.contains("PRIVATE"));
}

#[test]
fn watcher_event_drop_reaps_owned_process_and_joins_reader_despite_inherited_pipe() {
    // The shell can exit while its own child keeps stdout open. Drop must not
    // hang awaiting EOF or abandon a blocked reader thread in either case.
    for body in [
        "printf 'dark\\n'; exec /bin/sleep 20",
        "/bin/sleep 20 & printf 'dark\\n'; exit 0",
    ] {
        let source = source(body, Format::Macos);
        let pid = source._native.as_ref().unwrap().pid();
        changed(&source);
        let started = Instant::now();
        drop(source);
        assert!(started.elapsed() < Duration::from_secs(3));
        // Read-only reap check for this exact child, never signal a saved PID.
        let mut status = 0;
        assert_eq!(
            unsafe { libc::waitpid(pid as i32, &mut status, libc::WNOHANG) },
            -1
        );
        assert_eq!(
            std::io::Error::last_os_error().raw_os_error(),
            Some(libc::ECHILD)
        );
    }
}

#[test]
fn portal_watch_managed_source_reports_ready_only_after_subscription_and_joins_on_drop() {
    use crate::platform::portal::test_bus::{Bus, Mode, INTERFACE, OWNER, PATH};
    for fail in [false, true] {
        let mut bus = Bus::new(Mode::Normal);
        let connection = bus.take_client();
        let source = Source::portal(move || std::future::ready(Ok(connection))).unwrap();
        assert_eq!(bus.matches.load(std::sync::atomic::Ordering::SeqCst), 2);
        let message = zbus::Message::signal(PATH, INTERFACE, "SettingChanged")
            .unwrap()
            .sender(OWNER)
            .unwrap()
            .build(&(
                "org.freedesktop.appearance",
                "color-scheme",
                zbus::zvariant::Value::U32(1),
            ))
            .unwrap();
        bus.send(&message);
        changed(&source);
        if fail {
            for _ in 0..128 {
                bus.send(&message);
            }
            bus.owner_change("org.freedesktop.DBus", "");
            let deadline = Instant::now() + Duration::from_secs(3);
            loop {
                assert!(
                    Instant::now() < deadline,
                    "failure must not hide behind pending changes"
                );
                if let Event::Failed(message) =
                    source.events.recv_timeout(Duration::from_secs(3)).unwrap()
                {
                    assert!(message.contains("owner disappeared"), "{message}");
                    break;
                }
            }
        }
        let started = Instant::now();
        drop(source);
        assert!(started.elapsed() < Duration::from_secs(2));
        bus.assert_client_closed();
    }
}

#[test]
fn portal_watch_managed_startup_failure_cleans_up_before_returning() {
    use crate::platform::portal::test_bus::{Bus, Mode};
    for mode in [Mode::Denied, Mode::StalledSettings] {
        let mut bus = Bus::new(mode);
        let connection = bus.take_client();
        let failure = match Source::portal(move || std::future::ready(Ok(connection))) {
            Ok(_) => panic!("failed subscription must not report a ready source"),
            Err(error) => error.to_string(),
        };
        assert!(!failure.contains("PRIVATE"));
        assert!(
            failure.contains("access denied") || failure.contains("startup timed out"),
            "{failure}"
        );
        bus.assert_client_closed();
    }
}
