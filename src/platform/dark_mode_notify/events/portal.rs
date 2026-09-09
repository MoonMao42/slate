//! Own and join the Portal worker. A private cancellation socket wakes it during
//! connection, subscription or idle listening, without waiting for a new signal.
use super::{error, queue, Event, Result};
use futures_lite::io::AsyncReadExt;
use std::{
    future::Future, net::Shutdown, os::unix::net::UnixStream, sync::mpsc, thread::JoinHandle,
    time::Duration,
};

pub(super) struct PortalSource {
    cancel: UnixStream,
    worker: Option<JoinHandle<()>>,
}

impl Drop for PortalSource {
    fn drop(&mut self) {
        let _ = self.cancel.shutdown(Shutdown::Both);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

impl PortalSource {
    pub(super) fn spawn<F>(
        connect: impl FnOnce() -> F + Send + 'static,
        sender: queue::Sender,
    ) -> Result<Self>
    where
        F: Future<Output = zbus::Result<zbus::Connection>>,
    {
        let (cancel, listen) = UnixStream::pair()?;
        let mut listen = async_io::Async::new(listen)?;
        let (ready, startup) = mpsc::sync_channel(1);
        let worker = std::thread::Builder::new()
            .name("slate-portal-events".into())
            .spawn(move || {
                let result = async_io::block_on(crate::platform::portal::watch::run(
                    connect(),
                    async {
                        let mut byte = [0];
                        let _ = listen.read(&mut byte).await;
                    },
                    || {
                        ready
                            .try_send(Ok(()))
                            .map_err(|_| error("Watcher startup receiver closed"))
                    },
                    |_| {
                        sender
                            .send(Event::Changed)
                            .map_err(|_| error("Watcher receiver closed"))
                    },
                ));
                if let Err(error) = result {
                    let message = error.to_string();
                    let _ = ready.try_send(Err(message.clone()));
                    let _ = sender.send(Event::Failed(message));
                }
            })?;
        let source = Self {
            cancel,
            worker: Some(worker),
        };
        // The protocol has a 2s startup deadline; this is a separate guard for
        // delivering startup status. Any failure drops/cancels/joins the worker.
        match startup.recv_timeout(Duration::from_secs(3)) {
            Ok(Ok(())) => Ok(source),
            Ok(Err(message)) => Err(error(message)),
            Err(_) => Err(error("Portal watcher did not confirm its subscription")),
        }
    }
}
