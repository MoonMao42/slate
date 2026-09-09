//! Optional startup hints, never a native runtime/configuration validator.
//! Unsafe/unreadable sources simply provide no hint. This does not make them
//! safe write targets: setup's preflight and recovery checks still apply.
use crate::config::file_read::{self, Links};
use crate::env::SlateEnv;
use std::path::Path;

pub(super) fn read_hint(env: &SlateEnv, path: &Path, limit: u64) -> Option<Vec<u8>> {
    let resolved;
    let path = if env.session().is_isolated() {
        let home = std::fs::canonicalize(env.home()).ok()?;
        resolved = std::fs::canonicalize(path).ok()?;
        if !resolved.starts_with(home) {
            return None;
        }
        // Read the checked destination, not a final symlink that can be retargeted
        // after resolution. External directory mutation is still not locked out.
        resolved.as_path()
    } else {
        path
    };
    file_read::read(path, limit, Links::Follow)
        .ok()
        .flatten()
        .map(|source| source.bytes)
}

pub(super) fn theme_hint(env: &SlateEnv) -> Option<String> {
    let bytes = read_hint(
        env,
        &env.managed_file("current"),
        file_read::MAX_STATE_BYTES,
    )?;
    let id = std::str::from_utf8(&bytes).ok()?.trim();
    // Unknown printable IDs can be shown as saved state, but control data must
    // not reach the wizard's labels. This does not claim the theme is installed.
    if id.is_empty()
        || id
            .chars()
            .any(|c| c.is_control() || matches!(c, '\u{2028}' | '\u{2029}'))
    {
        return None;
    }
    Some(id.to_owned())
}
