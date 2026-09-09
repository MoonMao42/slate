//! Record removal has two distinct failure boundaries: unlink and directory sync.
use super::*;

pub(super) fn remove_record(env: &SlateEnv) -> Result<()> {
    fs::remove_file(record_path(env)).map_err(|error| {
        std::io::Error::new(
            error.kind(),
            format!("Recovery record could not be removed: {error}"),
        )
    })?;
    File::open(env.slate_cache_dir()).and_then(|file| file.sync_all()).map_err(|error| {
        std::io::Error::new(error.kind(), format!("Recovery record was removed, but syncing its directory failed: {error}. Removal durability is unconfirmed"))
    })?;
    Ok(())
}

pub(super) fn after_restore<T>(result: Result<T>) -> Result<T> {
    with_outcome(
        result,
        "Preview files were restored and were not rolled back.",
    )
}

pub(super) fn after_discard(result: Result<()>) -> Result<()> {
    with_outcome(
        result,
        "Current config files were not changed. No automatic rollback was attempted.",
    )
}

fn with_outcome<T>(result: Result<T>, outcome: &str) -> Result<T> {
    result.map_err(|error| {
        let detail = crate::cli::file_output::terminal_text(&error.to_string());
        let message = format!("{outcome} Recovery finalization needs attention: {detail}. Run `slate recover --dry-run` before proceeding.");
        match error {
            SlateError::IOError(error) => SlateError::IOError(std::io::Error::new(error.kind(), message)),
            _ => invalid(&message),
        }
    })
}
