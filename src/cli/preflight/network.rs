//! Advisory DNS only: bounded UI wait, at most one outstanding resolver per process.
use std::sync::{mpsc, Mutex};
use std::time::Duration;

struct Probe {
    pending: Mutex<Option<mpsc::Receiver<bool>>>,
}

impl Probe {
    fn check(
        &self,
        wait: Duration,
        resolve: impl FnOnce() -> bool + Send + 'static,
    ) -> Option<bool> {
        // Another caller already checking DNS must not make this caller wait too.
        let mut pending = self.pending.try_lock().ok()?;
        if pending.is_none() {
            let (sender, receiver) = mpsc::channel();
            std::thread::Builder::new()
                .name("slate-dns".into())
                .spawn(move || {
                    let _ = sender.send(resolve());
                })
                .ok()?;
            *pending = Some(receiver);
        }
        match pending.as_ref()?.recv_timeout(wait) {
            Ok(value) => {
                *pending = None;
                Some(value)
            }
            Err(mpsc::RecvTimeoutError::Timeout) => None,
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                *pending = None;
                None
            }
        }
    }
}

pub(super) fn check() -> Option<bool> {
    static PROBE: Probe = Probe {
        pending: Mutex::new(None),
    };
    PROBE.check(Duration::from_millis(300), || {
        std::net::ToSocketAddrs::to_socket_addrs("github.com:443")
            .is_ok_and(|mut addresses| addresses.next().is_some())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timeout_reuses_pending_lookup_and_keeps_unknown_distinct_from_failure() {
        let probe = Probe {
            pending: Mutex::new(None),
        };
        let (release, hold) = mpsc::channel();
        assert_eq!(
            probe.check(Duration::ZERO, move || {
                hold.recv().unwrap();
                false
            }),
            None
        );
        assert_eq!(
            probe.check(Duration::ZERO, || panic!("duplicate DNS lookup")),
            None
        );
        release.send(()).unwrap();
        assert_eq!(
            probe.check(Duration::from_secs(2), || panic!(
                "pending lookup was discarded"
            )),
            Some(false)
        );
        assert_eq!(probe.check(Duration::from_secs(2), || true), Some(true));
    }

    #[test]
    fn contention_and_disconnected_workers_do_not_report_offline() {
        let probe = Probe {
            pending: Mutex::new(None),
        };
        let held = probe.pending.lock().unwrap();
        assert_eq!(
            probe.check(Duration::ZERO, || panic!("unexpected lookup")),
            None
        );
        drop(held);
        let (sender, receiver) = mpsc::channel();
        drop(sender);
        *probe.pending.lock().unwrap() = Some(receiver);
        assert_eq!(
            probe.check(Duration::ZERO, || panic!("unexpected lookup")),
            None
        );
        assert!(probe.pending.lock().unwrap().is_none());
    }
}
