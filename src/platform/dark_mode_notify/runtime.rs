//! Per-profile watcher ownership. No process enumeration or PID-based stopping.
use crate::config::state_files::atomic_write_synced_mode;
use crate::config::write_guard::{check_private_file, try_lock};
use crate::env::SlateEnv;
use crate::error::{Result, SlateError};
use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions};
use std::io::Read;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[derive(Clone)]
pub(super) struct Profile {
    pub directory: PathBuf,
    key: String,
}

#[derive(Serialize, Deserialize)]
struct Instance {
    version: u8,
    profile: String,
    token: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum ExitKind {
    Stopped,
    Failed,
}

#[derive(Serialize, Deserialize)]
struct Completion {
    instance: Instance,
    outcome: ExitKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeState {
    Absent,
    Starting,
    Ready,
    Stopping,
    Stopped,
    Failed,
    Stale,
    Changing,
    Unreadable,
}

#[derive(Debug, Serialize)]
pub struct RuntimeInspection {
    pub state: RuntimeState,
    pub lock_held: Option<bool>,
    pub directory: Option<PathBuf>,
    pub log_path: Option<PathBuf>,
    pub message: &'static str,
}

impl RuntimeInspection {
    pub fn inspect(env: &SlateEnv) -> Self {
        let profile = Profile::new(env).ok();
        let result = profile.as_ref().map(|profile| profile.inspection());
        let (state, lock_held) = match result {
            Some(Ok(value)) => value,
            _ => (RuntimeState::Unreadable, None),
        };
        Self {
            state, lock_held,
            directory: profile.as_ref().map(|profile| profile.directory.clone()),
            log_path: profile.as_ref().map(|profile| profile.directory.join("watcher.log")),
            message: match state {
                RuntimeState::Absent => "No managed watcher is running and no instance record was found.",
                RuntimeState::Starting => "The lifetime lock is held, but startup is not acknowledged yet.",
                RuntimeState::Ready => "The managed event loop acknowledged startup; this does not prove theme application succeeded.",
                RuntimeState::Stopping => "The current instance has a stop request or is finishing its exit.",
                RuntimeState::Stopped => "The last recorded instance stopped normally; no lifetime lock is held.",
                RuntimeState::Failed => "The last recorded instance exited with a failure; inspect its private log.",
                RuntimeState::Stale => "No lifetime lock is held and the last instance has no matching exit record; its exit is unconfirmed.",
                RuntimeState::Changing => "Watcher ownership changed during inspection; inspect again for a stable result.",
                RuntimeState::Unreadable => "Watcher ownership/control files are unavailable, unsafe, or invalid; no running state is inferred.",
            },
        }
    }
}

fn error(message: impl Into<String>) -> SlateError {
    SlateError::PlatformError(message.into())
}

fn resolved(path: &Path) -> Result<PathBuf> {
    for parent in path.ancestors() {
        match fs::canonicalize(parent) {
            Ok(root) => {
                let suffix = path.strip_prefix(parent).expect("ancestor");
                return Ok(if suffix.as_os_str().is_empty() {
                    root
                } else {
                    root.join(suffix)
                });
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => continue,
            Err(err) => return Err(err.into()),
        }
    }
    Err(error("Cannot resolve watcher profile"))
}

impl Profile {
    pub fn new(env: &SlateEnv) -> Result<Self> {
        let config = resolved(env.config_dir())?;
        let key = uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_URL, config.as_os_str().as_bytes())
            .to_string();
        Ok(Self {
            directory: env.slate_cache_dir().join("watchers").join(&key),
            key,
        })
    }

    fn validate_directory(&self, create: bool) -> Result<bool> {
        // Refuse links at both runtime-specific levels. Never chmod an existing
        // user directory to make it pass validation.
        for path in [self.directory.parent().expect("watchers"), &self.directory] {
            match fs::symlink_metadata(path) {
                Err(err) if err.kind() == std::io::ErrorKind::NotFound && !create => {
                    return Ok(false)
                }
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                    fs::DirBuilder::new()
                        .recursive(true)
                        .mode(0o700)
                        .create(path)?;
                }
                Err(err) => return Err(err.into()),
                Ok(_) => {}
            }
            let meta = fs::symlink_metadata(path)?;
            if !meta.is_dir()
                || meta.uid() != unsafe { libc::geteuid() }
                || meta.mode() & 0o077 != 0
            {
                return Err(error(
                    "Watcher runtime directories must be private, owned directories (not links)",
                ));
            }
        }
        Ok(true)
    }

    fn lock(&self, create: bool) -> Result<Option<File>> {
        if !self.validate_directory(create)? {
            return Ok(None);
        }
        let file = match OpenOptions::new()
            .read(true)
            .write(true)
            .create(create)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(self.directory.join("instance.lock"))
        {
            Ok(file) => file,
            Err(err) if !create && err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(err) => return Err(err.into()),
        };
        check_private_file(&file)?;
        Ok(Some(file))
    }

    pub fn is_running(&self) -> Result<bool> {
        let Some(file) = self.lock(false)? else {
            return Ok(false);
        };
        Ok(!try_lock(&file)?)
    }

    fn read(&self, name: &str) -> Result<Option<Vec<u8>>> {
        let file = match OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK)
            .open(self.directory.join(name))
        {
            Ok(file) => file,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(err) => return Err(err.into()),
        };
        check_private_file(&file)?;
        let mut bytes = Vec::new();
        file.take(4097).read_to_end(&mut bytes)?;
        if bytes.len() > 4096 {
            return Err(error("Watcher control record is too large"));
        }
        Ok(Some(bytes))
    }

    fn instance(&self) -> Result<Option<Instance>> {
        let Some(bytes) = self.read("instance.json")? else {
            return Ok(None);
        };
        let instance: Instance = serde_json::from_slice(&bytes)
            .map_err(|_| error("Watcher control record is invalid; no process was signalled"))?;
        if instance.version != 1
            || instance.profile != self.key
            || uuid::Uuid::parse_str(&instance.token).is_err()
        {
            return Err(error("Watcher control record does not match this profile"));
        }
        Ok(Some(instance))
    }

    fn inspection(&self) -> Result<(RuntimeState, Option<bool>)> {
        let running = self.is_running()?;
        let instance = self.instance()?;
        let state = match instance.as_ref() {
            None if running => RuntimeState::Starting,
            None => RuntimeState::Absent,
            Some(instance) => {
                let completed = self
                    .read("exit.json")?
                    .map(|bytes| {
                        serde_json::from_slice::<Completion>(&bytes)
                            .map_err(|_| error("Watcher exit record is invalid"))
                    })
                    .transpose()?;
                let completed = completed.filter(|completion| {
                    completion.instance.version == 1
                        && completion.instance.profile == self.key
                        && completion.instance.token == instance.token
                });
                if !running {
                    match completed.map(|completion| completion.outcome) {
                        Some(ExitKind::Stopped) => RuntimeState::Stopped,
                        Some(ExitKind::Failed) => RuntimeState::Failed,
                        None => RuntimeState::Stale,
                    }
                } else if completed.is_some()
                    || self
                        .read("stop")?
                        .is_some_and(|bytes| bytes == instance.token.as_bytes())
                {
                    RuntimeState::Stopping
                } else if self
                    .read("ready")?
                    .is_some_and(|bytes| bytes == instance.token.as_bytes())
                {
                    RuntimeState::Ready
                } else {
                    RuntimeState::Starting
                }
            }
        };
        // Do not hold a probe lock across IO: that would make a concurrent
        // watcher mistake this read-only inspection for an existing owner.
        if self.is_running()? != running
            || self.instance()?.as_ref().map(|value| &value.token)
                != instance.as_ref().map(|value| &value.token)
        {
            return Ok((RuntimeState::Changing, None));
        }
        Ok((state, Some(running)))
    }

    pub fn stop(&self) -> Result<()> {
        if !self.is_running()? {
            return Ok(());
        }
        let deadline = Instant::now() + Duration::from_secs(4);
        let instance = loop {
            if !self.is_running()? {
                return Ok(());
            }
            if let Some(instance) = self.instance()? {
                break instance;
            }
            if Instant::now() >= deadline {
                return Err(error(
                    "Watcher has no control record; no process was signalled",
                ));
            }
            std::thread::sleep(Duration::from_millis(25));
        };
        atomic_write_synced_mode(
            &self.directory.join("stop"),
            instance.token.as_bytes(),
            Some(0o600),
        )?;
        loop {
            if !self.is_running()? {
                return Ok(());
            }
            // A new generation is not the instance we were asked to stop.
            if self
                .instance()?
                .is_some_and(|current| current.token != instance.token)
            {
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err(error(
                    "Watcher did not acknowledge its stop request; no process was signalled",
                ));
            }
            std::thread::sleep(Duration::from_millis(25));
        }
    }

    pub fn claim(&self) -> Result<Option<Lease>> {
        let file = self.lock(true)?.expect("created lock");
        if !try_lock(&file)? {
            return Ok(None);
        }
        let nonce = format!(
            "{}-{:?}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
        );
        let token = uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_OID, nonce.as_bytes()).to_string();
        let instance = Instance {
            version: 1,
            profile: self.key.clone(),
            token: token.clone(),
        };
        atomic_write_synced_mode(
            &self.directory.join("instance.json"),
            &serde_json::to_vec(&instance)?,
            Some(0o600),
        )?;
        Ok(Some(Lease {
            profile: self.clone(),
            token,
            _file: file,
        }))
    }

    pub fn start(&self, command: &mut Command) -> Result<()> {
        let mut child = BackgroundChild(if self.is_running()? {
            None
        } else {
            self.validate_directory(true)?;
            let log = OpenOptions::new()
                .create(true)
                .append(true)
                .mode(0o600)
                .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
                .open(self.directory.join("watcher.log"))?;
            check_private_file(&log)?;
            // Do not inherit the invoking terminal/runner's session and
            // process group. Only async-signal-safe libc calls after fork.
            unsafe {
                command.pre_exec(|| {
                    // Command can be reused; an earlier registered hook may
                    // already have established this child's own session.
                    if libc::getsid(0) != libc::getpid() && libc::setsid() < 0 {
                        return Err(std::io::Error::last_os_error());
                    }
                    Ok(())
                });
            }
            Some(
                command
                    .stdin(std::process::Stdio::null())
                    .stdout(std::process::Stdio::null())
                    .stderr(log)
                    .spawn()?,
            )
        });
        let deadline = Instant::now() + Duration::from_secs(4);
        loop {
            let running = self.is_running()?;
            if running {
                if let Some(instance) = self.instance()? {
                    if self
                        .read("ready")?
                        .is_some_and(|bytes| bytes == instance.token.as_bytes())
                    {
                        return Ok(());
                    }
                }
            }
            if let Some(owned) = child.0.as_mut() {
                if let Some(status) = owned.try_wait()? {
                    child.0 = None;
                    if !status.success() || !running {
                        return Err(error(format!(
                            "Watcher exited before startup completed ({status}); inspect {}",
                            self.directory.join("watcher.log").display()
                        )));
                    }
                }
            } else if !running {
                return Err(error("Watcher stopped before startup was confirmed"));
            }
            if Instant::now() >= deadline {
                return Err(error(format!(
                    "Watcher startup was not confirmed; inspect {}",
                    self.directory.join("watcher.log").display()
                )));
            }
            std::thread::sleep(Duration::from_millis(25));
        }
    }
}

struct BackgroundChild(Option<std::process::Child>);
impl Drop for BackgroundChild {
    fn drop(&mut self) {
        if let Some(mut child) = self.0.take() {
            // Reap on eventual exit when invoked from a long-lived hub. A
            // startup timeout is not proof the process stopped, so never kill it.
            std::thread::spawn(move || {
                let _ = child.wait();
            });
        }
    }
}

pub(super) struct Lease {
    profile: Profile,
    token: String,
    _file: File,
}

impl Lease {
    pub fn finish(&self, outcome: ExitKind) -> Result<()> {
        let completion = Completion {
            instance: Instance {
                version: 1,
                profile: self.profile.key.clone(),
                token: self.token.clone(),
            },
            outcome,
        };
        atomic_write_synced_mode(
            &self.profile.directory.join("exit.json"),
            &serde_json::to_vec(&completion)?,
            Some(0o600),
        )
    }
    pub fn ready(&self) -> Result<()> {
        atomic_write_synced_mode(
            &self.profile.directory.join("ready"),
            self.token.as_bytes(),
            Some(0o600),
        )
    }
    pub fn should_stop(&self) -> Result<bool> {
        Ok(self
            .profile
            .read("stop")?
            .is_some_and(|bytes| bytes == self.token.as_bytes()))
    }
}

impl Drop for Lease {
    fn drop(&mut self) {
        // Leave lock/control files in place. Only a live kernel lock counts as
        // running; generation matching makes old ready/stop records harmless.
    }
}
