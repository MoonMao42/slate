//! Bounded capture for native helpers. Uses nonblocking pipes and an owned
//! process group, with no reader threads left behind on timeout. The deadline
//! starts after spawn; filesystem/spawn and OS termination are not hard-bounded.
use std::io::{self, Read};
use std::os::fd::AsRawFd;
use std::os::unix::process::CommandExt;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

#[derive(Clone, Copy)]
pub(crate) struct Limits {
    pub timeout: Duration,
    pub max_output: usize,
}

pub(crate) enum Completion {
    Exited(ExitStatus),
    TimedOut,
    OutputLimit,
}

pub(crate) struct CapturedOutput {
    pub completion: Completion,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

struct OwnedChild(Child, bool);

impl Drop for OwnedChild {
    fn drop(&mut self) {
        if !self.1 {
            // Do not reap the leader before inherited pipes close: its PID then
            // cannot be reused while we still need to signal this owned group.
            unsafe {
                libc::kill(-(self.0.id() as i32), libc::SIGKILL);
            }
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
}

fn nonblocking(pipe: &impl AsRawFd) -> io::Result<()> {
    let fd = pipe.as_raw_fd();
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    if flags < 0 || unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

// One chunk per stream per poll prevents noisy output from starving the other
// stream, process completion or the deadline. The cap covers both streams.
fn drain(
    pipe: &mut impl Read,
    bytes: &mut Vec<u8>,
    total: &mut usize,
    max: usize,
) -> io::Result<(bool, bool)> {
    let mut buffer = [0; 8192];
    match pipe.read(&mut buffer) {
        Ok(0) => Ok((true, false)),
        Ok(count) => {
            let keep = count.min(max.saturating_sub(*total));
            bytes.extend_from_slice(&buffer[..keep]);
            *total += keep;
            Ok((false, keep < count))
        }
        Err(error)
            if matches!(
                error.kind(),
                io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
            ) =>
        {
            Ok((false, false))
        }
        Err(error) => Err(error),
    }
}

pub(crate) fn capture(command: &mut Command, limits: Limits) -> io::Result<CapturedOutput> {
    capture_with_hook(command, limits, || {})
}

/// The hook lets private fixtures signal readiness before a short test deadline.
/// Production callers supply no work here.
pub(crate) fn capture_with_hook(
    command: &mut Command,
    limits: Limits,
    after_spawn: impl FnOnce(),
) -> io::Result<CapturedOutput> {
    let child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0)
        .spawn()?;
    let mut child = OwnedChild(child, false);
    let mut stdout = child.0.stdout.take().expect("piped stdout");
    let mut stderr = child.0.stderr.take().expect("piped stderr");
    nonblocking(&stdout)?;
    nonblocking(&stderr)?;
    after_spawn();
    let started = Instant::now();
    let mut out = Vec::new();
    let mut err = Vec::new();
    let mut total = 0;
    let (mut out_done, mut err_done) = (false, false);
    let completion = loop {
        let mut limit = false;
        if !out_done {
            let result = drain(&mut stdout, &mut out, &mut total, limits.max_output)?;
            out_done = result.0;
            limit |= result.1;
        }
        if !err_done && !limit {
            let result = drain(&mut stderr, &mut err, &mut total, limits.max_output)?;
            err_done = result.0;
            limit |= result.1;
        }
        if limit {
            break Completion::OutputLimit;
        }
        if out_done && err_done {
            if let Some(status) = child.0.try_wait()? {
                child.1 = true;
                break Completion::Exited(status);
            }
        }
        if started.elapsed() >= limits.timeout {
            break Completion::TimedOut;
        }
        std::thread::sleep(Duration::from_millis(5));
    };
    Ok(CapturedOutput {
        completion,
        stdout: out,
        stderr: err,
    })
}
