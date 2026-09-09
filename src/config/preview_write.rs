//! Thread-scoped attribution of preview writes. Receipts contain intended bytes,
//! never arbitrary post-apply readback. Checks are cooperative, not a filesystem
//! transaction or an atomic lock against non-cooperating external editors.
use super::file_read::{self, Links, MAX_TOOL_CONFIG_BYTES};
use crate::error::{Result, SlateError};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::fs;
use std::marker::PhantomData;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::{Arc, Mutex, MutexGuard};

pub(crate) const MAX_STATE_BYTES: u64 = 16 * 1024 * 1024;

// Keep the existing preview-journal representation. Deliberately no Debug.
#[derive(Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) enum FileState {
    Absent,
    Present { bytes: Vec<u8>, mode: u32 },
}

impl FileState {
    fn size(&self) -> usize {
        match self {
            Self::Absent => 0,
            Self::Present { bytes, .. } => bytes.len(),
        }
    }
}

pub(crate) struct Entry {
    pub path: PathBuf,
    pub destination: PathBuf,
    pub expected: FileState,
}

struct State {
    entries: Vec<Entry>,
    failed: bool,
    closed: bool,
    reservations: BTreeMap<PathBuf, usize>,
}

thread_local! {
    static CURRENT: RefCell<Option<Arc<Mutex<State>>>> = const { RefCell::new(None) };
}

fn locked(state: &Mutex<State>) -> Result<MutexGuard<'_, State>> {
    state
        .lock()
        .map_err(|_| invalid("write receipts are unavailable"))
}

#[derive(Clone)]
pub(crate) struct Context(Arc<Mutex<State>>);

pub(crate) fn context() -> Option<Context> {
    CURRENT.with(|current| {
        current
            .borrow()
            .as_ref()
            .map(|state| Context(state.clone()))
    })
}

// These guards install thread-local state and must be dropped on that thread.
// Context alone is transferable to the explicitly joined adapter workers.
#[must_use = "Keep the attachment alive while this worker writes"]
pub(crate) struct Attachment {
    previous: Option<Arc<Mutex<State>>>,
    thread_bound: PhantomData<Rc<()>>,
}

impl Context {
    pub(crate) fn enter(&self) -> Attachment {
        Attachment {
            previous: CURRENT.with(|current| current.replace(Some(self.0.clone()))),
            thread_bound: PhantomData,
        }
    }
}

impl Drop for Attachment {
    fn drop(&mut self) {
        CURRENT.with(|current| {
            current.replace(self.previous.take());
        });
    }
}

fn invalid(reason: &str) -> SlateError {
    SlateError::InvalidConfig(format!("Preview write refused: {reason}. Close the picker, then inspect `slate recover --dry-run`."))
}

fn destination(path: &Path) -> Result<PathBuf> {
    match fs::symlink_metadata(path) {
        Ok(_) => fs::canonicalize(path).map_err(|_| invalid("cannot resolve target")),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            file_read::confirm_missing(path).map_err(|_| invalid("target or parent is unsafe"))?;
            file_read::directory_alias_target(path)
                .ok_or_else(|| invalid("cannot resolve target parent"))
        }
        Err(_) => Err(invalid("cannot inspect target")),
    }
}

fn read_state(path: &Path) -> Result<FileState> {
    file_read::read_with_metadata(path, MAX_TOOL_CONFIG_BYTES, Links::Reject)
        .map_err(|_| invalid("target cannot be read safely within the preview limit"))
        .map(|source| match source {
            Some((source, metadata)) => FileState::Present {
                bytes: source.bytes,
                mode: metadata.permissions().mode(),
            },
            None => FileState::Absent,
        })
}

#[must_use = "Keep write attribution active through adapter execution"]
pub(crate) struct Scope {
    state: Arc<Mutex<State>>,
    thread_bound: PhantomData<Rc<()>>,
}

impl Scope {
    pub(crate) fn begin(entries: Vec<Entry>) -> Result<Self> {
        let total: usize = entries.iter().map(|entry| entry.expected.size()).sum();
        if total as u64 > MAX_STATE_BYTES {
            return Err(invalid("captured write state exceeds 16 MiB"));
        }
        for (index, entry) in entries.iter().enumerate() {
            if entries[..index].iter().any(|other| {
                other.destination == entry.destination && other.expected != entry.expected
            }) {
                return Err(invalid("aliased preview targets have inconsistent state"));
            }
        }
        CURRENT.with(|current| {
            let mut current = current.borrow_mut();
            if current.is_some() {
                return Err(invalid("write attribution is already active"));
            }
            let state = Arc::new(Mutex::new(State {
                entries,
                failed: false,
                closed: false,
                reservations: BTreeMap::new(),
            }));
            *current = Some(state.clone());
            Ok(Self {
                state,
                thread_bound: PhantomData,
            })
        })
    }

    pub(crate) fn finish(self) -> Result<(Vec<FileState>, bool)> {
        let mut state = locked(&self.state)?;
        if !state.reservations.is_empty() {
            return Err(invalid("preview writers have not finished"));
        }
        // Seal while holding the receipt lock, before cloning the final state.
        state.closed = true;
        Ok((
            state
                .entries
                .iter()
                .map(|entry| entry.expected.clone())
                .collect(),
            state.failed,
        ))
    }
}

impl Drop for Scope {
    fn drop(&mut self) {
        self.state
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .closed = true;
        CURRENT.with(|current| {
            let mut current = current.borrow_mut();
            if current
                .as_ref()
                .is_some_and(|state| Arc::ptr_eq(state, &self.state))
            {
                *current = None;
            }
        });
    }
}

pub(crate) struct Ticket {
    state: Arc<Mutex<State>>,
    path: PathBuf,
    destination: PathBuf,
    expected: FileState,
    completed: bool,
}

impl Ticket {
    pub(crate) fn verify(&self) -> Result<()> {
        if destination(&self.path)? != self.destination {
            return Err(invalid("the write path was redirected"));
        }
        let state = locked(&self.state)?;
        if state.closed {
            return Err(invalid("preview write scope has ended"));
        }
        for entry in state
            .entries
            .iter()
            .filter(|entry| entry.destination == self.destination)
        {
            if destination(&entry.path)? != entry.destination {
                return Err(invalid("a preview path was redirected"));
            }
        }
        if read_state(&self.destination)? != self.expected {
            return Err(invalid(
                "target changed before publication; preserving the current contents",
            ));
        }
        Ok(())
    }

    pub(crate) fn committed(mut self, bytes: &[u8], mode: u32) -> Result<()> {
        let mut state = locked(&self.state)?;
        for entry in state
            .entries
            .iter_mut()
            .filter(|entry| entry.destination == self.destination)
        {
            entry.expected = FileState::Present {
                bytes: bytes.to_vec(),
                mode,
            };
        }
        state.reservations.remove(&self.destination);
        self.completed = true;
        Ok(())
    }
}

impl Drop for Ticket {
    fn drop(&mut self) {
        if !self.completed {
            if let Ok(mut state) = self.state.lock() {
                state.failed = true;
                state.reservations.remove(&self.destination);
            }
        }
    }
}

/// Called before opening a temporary output file and rechecked before rename.
pub(crate) fn prepare(path: &Path, length: usize) -> Result<Option<Ticket>> {
    let Some(state) = CURRENT.with(|current| current.borrow().clone()) else {
        return Ok(None);
    };
    let result = (|| {
        if length as u64 > MAX_TOOL_CONFIG_BYTES {
            return Err(invalid("output exceeds 8 MiB per file"));
        }
        let target = destination(path)?;
        let expected = {
            let mut state = locked(&state)?;
            if state.closed {
                return Err(invalid("preview write scope has ended"));
            }
            if state.reservations.contains_key(&target) {
                return Err(invalid("another preview writer is using this target"));
            }
            let matching = state
                .entries
                .iter()
                .find(|entry| entry.destination == target)
                .ok_or_else(|| invalid("target is outside the captured preview files"))?;
            let total: usize = state
                .entries
                .iter()
                .map(|entry| {
                    if entry.destination == target {
                        length
                    } else {
                        state
                            .reservations
                            .get(&entry.destination)
                            .copied()
                            .unwrap_or_else(|| entry.expected.size())
                    }
                })
                .sum();
            if total as u64 > MAX_STATE_BYTES {
                return Err(invalid("output exceeds 16 MiB per captured state"));
            }
            let expected = matching.expected.clone();
            state.reservations.insert(target.clone(), length);
            expected
        };
        let ticket = Ticket {
            state: state.clone(),
            path: path.to_owned(),
            destination: target,
            expected,
            completed: false,
        };
        ticket.verify()?;
        Ok(Some(ticket))
    })();
    if result.is_err() {
        if let Ok(mut state) = state.lock() {
            state.failed = true;
        }
    }
    result
}

/// Preserve legacy linked/in-place writes outside preview. Within a preview,
/// resolve the captured dotfile target and use the tracked atomic publisher.
pub(crate) fn write_legacy(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(context) = context() {
        let result = destination(path)
            .and_then(|target| super::state_files::atomic_write_synced(&target, bytes));
        if result.is_err() {
            if let Ok(mut state) = context.0.lock() {
                state.failed = true;
            }
        }
        result
    } else {
        fs::write(path, bytes).map_err(Into::into)
    }
}

#[cfg(test)]
mod tests;
