//! Preserve explicitly supplied stdout descriptors. assert_cmd::Command::assert
//! installs its own capture pipe and would invalidate these redirection tests.
use std::{
    io::{Read, Seek, SeekFrom},
    process::{Child, Command, Output, Stdio},
    time::{Duration, Instant},
};

struct OwnedChild(Child);

impl Drop for OwnedChild {
    fn drop(&mut self) {
        if !matches!(self.0.try_wait(), Ok(Some(_))) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
}

pub fn run(command: &mut Command) -> Output {
    // A private anonymous file avoids a blocked stderr pipe without replacing
    // stdout or leaving an unbounded reader thread. No caller/profile files.
    let mut stderr = tempfile::tempfile().unwrap();
    command.stderr(Stdio::from(stderr.try_clone().unwrap()));
    let mut child = OwnedChild(command.spawn().unwrap());
    let deadline = Instant::now() + Duration::from_secs(5);
    let status = loop {
        if let Some(status) = child.0.try_wait().unwrap() {
            break status;
        }
        assert!(
            Instant::now() < deadline,
            "redirected command exceeded deadline"
        );
        std::thread::sleep(Duration::from_millis(10));
    };
    stderr.seek(SeekFrom::Start(0)).unwrap();
    let mut bytes = Vec::new();
    stderr.take(65537).read_to_end(&mut bytes).unwrap();
    assert!(bytes.len() <= 65536, "unexpectedly large diagnostic stderr");
    Output {
        status,
        stdout: Vec::new(),
        stderr: bytes,
    }
}
