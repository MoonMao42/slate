//! Coalesce pending appearance invalidations, keeping terminal failure separate
//! so an event flood cannot allocate a backlog or hide a failed source.
use super::Event;
use std::{
    sync::{mpsc, Arc, Mutex},
    time::Duration,
};

type Failure = Arc<Mutex<Option<String>>>;

#[derive(Clone)]
pub(crate) struct Sender {
    wake: mpsc::SyncSender<()>,
    failure: Failure,
}

pub(crate) struct Receiver {
    wake: mpsc::Receiver<()>,
    failure: Failure,
}

pub(super) fn channel() -> (Sender, Receiver) {
    let (send, receive) = mpsc::sync_channel(1);
    let failure = Arc::new(Mutex::new(None));
    (
        Sender {
            wake: send,
            failure: failure.clone(),
        },
        Receiver {
            wake: receive,
            failure,
        },
    )
}

impl Sender {
    pub fn send(&self, event: Event) -> std::result::Result<(), ()> {
        if let Event::Failed(message) = event {
            self.failure
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .get_or_insert(message);
        }
        match self.wake.try_send(()) {
            Ok(()) | Err(mpsc::TrySendError::Full(())) => Ok(()),
            Err(mpsc::TrySendError::Disconnected(())) => Err(()),
        }
    }
}

impl Receiver {
    pub fn recv_timeout(&self, timeout: Duration) -> Result<Event, mpsc::RecvTimeoutError> {
        if let Some(message) = self.take_failure() {
            return Ok(Event::Failed(message));
        }
        let wake = self.wake.recv_timeout(timeout);
        // Recheck after waking: a failed source takes priority over a pending
        // change. Changes carry no state; application always rereads appearance.
        if let Some(message) = self.take_failure() {
            return Ok(Event::Failed(message));
        }
        wake.map(|()| Event::Changed)
    }

    fn take_failure(&self) -> Option<String> {
        self.failure
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take()
    }
}
