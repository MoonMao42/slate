//! Owned, cancellable native event reader. Long idle periods are valid; only
//! individual records are bounded. No detached blocking line-reader remains.
use super::{error, queue, Event, Result};
use std::{
    io::{self, Read},
    net::Shutdown,
    os::{
        fd::AsRawFd,
        unix::{net::UnixStream, process::CommandExt},
    },
    process::{Child, ChildStdout, Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread::JoinHandle,
};

const MAX_RECORD: usize = 1024;

#[derive(Clone, Copy)]
pub(super) enum Format {
    #[cfg(any(target_os = "macos", test))]
    Macos,
    #[cfg(any(target_os = "linux", test))]
    Gnome,
}

impl Format {
    fn changed(self, bytes: &[u8]) -> bool {
        let Ok(line) = std::str::from_utf8(bytes) else {
            return false;
        };
        match self {
            #[cfg(any(target_os = "macos", test))]
            Self::Macos => matches!(line.strip_suffix('\r').unwrap_or(line), "dark" | "light"),
            #[cfg(any(target_os = "linux", test))]
            Self::Gnome => {
                crate::platform::desktop::parse_gnome_color_scheme_output(line).is_some()
            }
        }
    }
}

struct OwnedChild(Child);

impl Drop for OwnedChild {
    fn drop(&mut self) {
        // Never reap the leader before signalling its owned group: its PID
        // cannot be reused while descendants may still hold our stdout pipe.
        unsafe {
            libc::kill(-(self.0.id() as i32), libc::SIGKILL);
        }
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

pub(super) struct NativeSource {
    child: Option<OwnedChild>,
    stopped: Arc<AtomicBool>,
    wake: UnixStream,
    reader: Option<JoinHandle<()>>,
}

impl Drop for NativeSource {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::Release);
        let _ = self.wake.shutdown(Shutdown::Both);
        drop(self.child.take());
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}

impl NativeSource {
    pub(super) fn spawn(
        mut command: Command,
        format: Format,
        sender: queue::Sender,
    ) -> Result<Self> {
        #[cfg(target_os = "linux")]
        {
            let parent = unsafe { libc::getpid() };
            unsafe {
                command.pre_exec(move || {
                    if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM as libc::c_ulong) != 0 {
                        return Err(io::Error::last_os_error());
                    }
                    if libc::getppid() != parent {
                        libc::_exit(1);
                    }
                    Ok(())
                });
            }
        }
        let mut child = OwnedChild(
            command
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .process_group(0)
                .spawn()?,
        );
        let stdout = child
            .0
            .stdout
            .take()
            .ok_or_else(|| error("Appearance source stdout is unavailable"))?;
        let fd = stdout.as_raw_fd();
        let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
        if flags < 0 || unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
            return Err(io::Error::last_os_error().into());
        }
        let stopped = Arc::new(AtomicBool::new(false));
        let (wake, cancellation) = UnixStream::pair()?;
        let stop_reader = stopped.clone();
        let reader = std::thread::Builder::new()
            .name("slate-appearance-events".into())
            .spawn(move || read_events(stdout, format, sender, &stop_reader, cancellation))?;
        Ok(Self {
            child: Some(child),
            stopped,
            wake,
            reader: Some(reader),
        })
    }

    #[cfg(test)]
    pub(super) fn pid(&self) -> u32 {
        self.child.as_ref().unwrap().0.id()
    }
}

fn read_events(
    mut stdout: ChildStdout,
    format: Format,
    sender: queue::Sender,
    stopped: &AtomicBool,
    cancellation: UnixStream,
) {
    let mut record = Vec::with_capacity(MAX_RECORD);
    let mut chunk = [0; 4096];
    while !stopped.load(Ordering::Acquire) {
        match stdout.read(&mut chunk) {
            Ok(0) => {
                // An unterminated last record is not an appearance event.
                let _ = sender.send(Event::Failed("Appearance event source exited".into()));
                return;
            }
            Ok(count) => {
                for &byte in &chunk[..count] {
                    if byte == b'\n' {
                        if format.changed(&record) && sender.send(Event::Changed).is_err() {
                            return;
                        }
                        record.clear();
                    } else if record.len() == MAX_RECORD {
                        let _ = sender.send(Event::Failed(format!(
                            "Appearance event record exceeded {MAX_RECORD} bytes; native output omitted"
                        )));
                        return;
                    } else {
                        record.push(byte);
                    }
                }
            }
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                match wait_for_output(&stdout, &cancellation) {
                    Ok(true) => {}
                    Ok(false) => return,
                    Err(e) => {
                        let _ = sender.send(Event::Failed(format!(
                            "Appearance event wait failed ({:?}); native output omitted",
                            e.kind()
                        )));
                        return;
                    }
                }
            }
            Err(e) => {
                let _ = sender.send(Event::Failed(format!(
                    "Appearance event read failed ({:?}); native output omitted",
                    e.kind()
                )));
                return;
            }
        }
    }
}

fn wait_for_output(stdout: &ChildStdout, cancellation: &UnixStream) -> io::Result<bool> {
    // A private socket wakes an idle reader on Drop, even when an inherited
    // stdout remains open. No periodic timer or busy polling is needed.
    let mut fds = [
        libc::pollfd {
            fd: stdout.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        },
        libc::pollfd {
            fd: cancellation.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        },
    ];
    loop {
        let result = unsafe { libc::poll(fds.as_mut_ptr(), fds.len() as libc::nfds_t, -1) };
        if result >= 0 {
            return Ok(fds[1].revents == 0);
        }
        let e = io::Error::last_os_error();
        if e.kind() != io::ErrorKind::Interrupted {
            return Err(e);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn watcher_event_idle_wait_is_woken_without_waiting_for_child_exit() {
        let mut child = OwnedChild(
            Command::new("/bin/sleep")
                .arg("10")
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .process_group(0)
                .spawn()
                .unwrap(),
        );
        let stdout = child.0.stdout.take().unwrap();
        let (wake, cancellation) = UnixStream::pair().unwrap();
        let (send, receive) = std::sync::mpsc::channel();
        let reader = std::thread::spawn(move || {
            let result = wait_for_output(&stdout, &cancellation);
            let _ = send.send(result);
        });
        wake.shutdown(Shutdown::Both).unwrap();
        assert!(!receive
            .recv_timeout(std::time::Duration::from_secs(2))
            .unwrap()
            .unwrap());
        assert!(
            child.0.try_wait().unwrap().is_none(),
            "cancellation must not need EOF"
        );
        reader.join().unwrap();
    }
}
