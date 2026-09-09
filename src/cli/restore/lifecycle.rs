//! File restoration and watcher lifecycle have separate success boundaries.
use crate::error::{Result, SlateError};

pub(super) fn sync(
    isolated: bool,
    enabled: impl FnOnce() -> Result<bool>,
    stop: impl FnOnce() -> Result<()>,
    start: impl FnOnce() -> Result<()>,
) -> Result<()> {
    if isolated {
        return Ok(());
    }
    // Unknown settings must not be interpreted as disabled. Inspect before
    // touching processes, and never start a replacement after a failed stop.
    let enabled = enabled().map_err(|_| {
        SlateError::InvalidConfig(
            "Restored auto-theme settings could not be read; the watcher was left unchanged."
                .into(),
        )
    })?;
    stop().map_err(|_| {
        SlateError::InvalidConfig(
            "Auto-theme watcher stop was not confirmed; no replacement was started.".into(),
        )
    })?;
    if enabled {
        start().map_err(|_| {
            SlateError::InvalidConfig(
                "Auto-theme watcher start was not confirmed after stopping the previous watcher."
                    .into(),
            )
        })?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    #[test]
    fn watcher_failures_stop_at_the_failed_stage_and_never_assume_disabled() {
        for (isolated, enabled_value, failure, expected) in [
            (true, true, "read", vec![]),
            (false, false, "", vec!["read", "stop"]),
            (false, true, "", vec!["read", "stop", "start"]),
            (false, true, "read", vec!["read"]),
            (false, true, "stop", vec!["read", "stop"]),
            (false, true, "start", vec!["read", "stop", "start"]),
        ] {
            let calls = RefCell::new(Vec::new());
            let step = |name| {
                calls.borrow_mut().push(name);
                if name == failure {
                    Err(SlateError::InvalidConfig("PRIVATE_FIXTURE".into()))
                } else {
                    Ok(())
                }
            };
            let result = sync(
                isolated,
                || {
                    step("read")?;
                    Ok(enabled_value)
                },
                || step("stop"),
                || step("start"),
            );
            assert_eq!(*calls.borrow(), expected);
            assert_eq!(result.is_ok(), isolated || failure.is_empty());
            if let Err(error) = result {
                assert!(!error.to_string().contains("PRIVATE_FIXTURE"));
                assert!(error.to_string().contains(match failure {
                    "read" => "could not be read",
                    "stop" => "no replacement was started",
                    _ => "start was not confirmed",
                }));
            }
        }
    }
}
