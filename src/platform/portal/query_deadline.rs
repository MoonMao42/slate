//! Deadline for finite Portal queries, not interactive screenshots or streams.
use crate::error::{Result, SlateError};
use std::{future::Future, time::Duration};

pub(super) const QUERY_TIMEOUT: Duration = Duration::from_secs(2);

pub(super) fn run<T>(future: impl Future<Output = Result<T>>) -> Result<T> {
    run_with_timeout(future, QUERY_TIMEOUT)
}

fn run_with_timeout<T>(future: impl Future<Output = Result<T>>, timeout: Duration) -> Result<T> {
    async_io::block_on(futures_lite::future::or(future, async {
        async_io::Timer::after(timeout).await;
        Err(SlateError::PlatformError(format!(
            "Portal settings query timed out after {} ms",
            timeout.as_millis()
        )))
    }))
}

pub(super) fn settings_unavailable(error: &zbus::Error) -> bool {
    match error {
        zbus::Error::InputOutput(err) => matches!(
            err.kind(),
            std::io::ErrorKind::NotFound | std::io::ErrorKind::ConnectionRefused
        ),
        zbus::Error::MethodError(name, _, _) => matches!(
            name.as_str(),
            "org.freedesktop.DBus.Error.ServiceUnknown"
                | "org.freedesktop.DBus.Error.NameHasNoOwner"
                | "org.freedesktop.DBus.Error.UnknownObject"
                | "org.freedesktop.DBus.Error.UnknownInterface"
                | "org.freedesktop.DBus.Error.UnknownMethod"
                | "org.freedesktop.portal.Error.NotFound"
        ),
        zbus::Error::FDO(err) => matches!(
            err.as_ref(),
            zbus::fdo::Error::ServiceUnknown(_)
                | zbus::fdo::Error::NameHasNoOwner(_)
                | zbus::fdo::Error::UnknownObject(_)
                | zbus::fdo::Error::UnknownInterface(_)
                | zbus::fdo::Error::UnknownMethod(_)
        ),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::Cell, time::Instant};

    #[test]
    fn appearance_portal_absence_does_not_swallow_access_or_protocol_errors() {
        for kind in [
            std::io::ErrorKind::NotFound,
            std::io::ErrorKind::ConnectionRefused,
        ] {
            assert!(settings_unavailable(&std::io::Error::from(kind).into()));
        }
        assert!(settings_unavailable(&zbus::Error::FDO(Box::new(
            zbus::fdo::Error::ServiceUnknown("fixture".into())
        ))));
        for error in [
            std::io::Error::from(std::io::ErrorKind::PermissionDenied).into(),
            zbus::Error::Address("PRIVATE invalid address".into()),
            zbus::Error::InvalidReply,
            zbus::Error::FDO(Box::new(zbus::fdo::Error::AccessDenied(
                "PRIVATE denied".into(),
            ))),
        ] {
            assert!(!settings_unavailable(&error));
        }
    }

    #[test]
    fn appearance_portal_deadline_keeps_completed_results() {
        assert_eq!(run(async { Ok(42) }).unwrap(), 42);
        let error =
            run::<()>(async { Err(SlateError::PlatformError("query fixture failed".into())) })
                .unwrap_err()
                .to_string();
        assert!(error.contains("query fixture failed"));
        assert_eq!(QUERY_TIMEOUT, Duration::from_secs(2));
    }

    #[test]
    fn appearance_portal_deadline_drops_an_incomplete_query() {
        struct OnDrop<'a>(&'a Cell<bool>);
        impl Drop for OnDrop<'_> {
            fn drop(&mut self) {
                self.0.set(true);
            }
        }
        let dropped = Cell::new(false);
        let guard = OnDrop(&dropped);
        let future = async move {
            let _guard = guard;
            std::future::pending::<Result<()>>().await
        };
        let started = Instant::now();
        let error = run_with_timeout(future, Duration::from_millis(100))
            .unwrap_err()
            .to_string();
        assert!(error.contains("timed out after 100 ms"));
        assert!(started.elapsed() < Duration::from_secs(3));
        assert!(
            dropped.get(),
            "do not leave the query running in a detached worker"
        );
    }
}
