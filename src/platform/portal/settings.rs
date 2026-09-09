//! Finite Settings queries shared by Linux production and private D-Bus tests.
use super::{interfaces::PortalSettingsProxy, query_deadline};
use crate::error::{Result, SlateError};
use std::future::Future;
use zbus::{
    zvariant::{OwnedValue, Value},
    Connection,
};

pub(super) const APPEARANCE_NAMESPACE: &str = "org.freedesktop.appearance";
pub(super) const COLOR_SCHEME_KEY: &str = "color-scheme";

#[cfg(target_os = "linux")]
pub(super) fn version() -> Result<u32> {
    query_deadline::run(version_query(Connection::session()))
}

#[cfg(target_os = "linux")]
pub(super) fn read_color_scheme() -> Result<Option<u32>> {
    // One budget includes connecting, ReadOne, and the optional legacy Read.
    query_deadline::run(read_query(Connection::session()))
}

async fn settings_proxy(connection: &Connection) -> zbus::Result<PortalSettingsProxy<'_>> {
    PortalSettingsProxy::builder(connection)
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await
}

async fn version_query(connection: impl Future<Output = zbus::Result<Connection>>) -> Result<u32> {
    let connection = connection.await.map_err(|e| query_error("connection", e))?;
    let proxy = settings_proxy(&connection)
        .await
        .map_err(|e| query_error("proxy", e))?;
    proxy.version().await.map_err(|e| query_error("version", e))
}

async fn read_query(
    connection: impl Future<Output = zbus::Result<Connection>>,
) -> Result<Option<u32>> {
    let connection = match connection.await {
        Ok(connection) => connection,
        Err(error) => return unavailable_or_error("connection", error),
    };
    let proxy = match settings_proxy(&connection).await {
        Ok(proxy) => proxy,
        Err(error) => return unavailable_or_error("proxy", error),
    };
    let (value, legacy) = match proxy.read_one(APPEARANCE_NAMESPACE, COLOR_SCHEME_KEY).await {
        Ok(value) => (value, false),
        Err(error) if unknown_method(&error) => {
            match proxy
                .read_legacy(APPEARANCE_NAMESPACE, COLOR_SCHEME_KEY)
                .await
            {
                Ok(value) => (value, true),
                Err(error) => return unavailable_or_error("Read fallback", error),
            }
        }
        Err(error) => return unavailable_or_error("ReadOne", error),
    };
    decode_scheme(&value, legacy).map(Some)
}

fn unknown_method(error: &zbus::Error) -> bool {
    match error {
        zbus::Error::MethodError(name, _, _) => {
            name.as_str() == "org.freedesktop.DBus.Error.UnknownMethod"
        }
        zbus::Error::FDO(error) => matches!(error.as_ref(), zbus::fdo::Error::UnknownMethod(_)),
        _ => false,
    }
}

fn unavailable_or_error(stage: &str, error: zbus::Error) -> Result<Option<u32>> {
    if query_deadline::settings_unavailable(&error) {
        Ok(None)
    } else {
        Err(query_error(stage, error))
    }
}

fn decode_scheme(value: &OwnedValue, legacy: bool) -> Result<u32> {
    // Deserializing OwnedValue already removes the method's outer variant.
    // The deprecated Read ABI has exactly one additional variant layer.
    let scheme = match (&**value, legacy) {
        (Value::U32(value), false) => Some(*value),
        (Value::Value(inner), true) => match inner.as_ref() {
            Value::U32(value) => Some(*value),
            _ => None,
        },
        _ => None,
    };
    scheme.ok_or_else(|| {
        SlateError::PlatformError(format!(
            "Portal Settings {} returned an invalid color-scheme type; response details omitted",
            if legacy { "Read fallback" } else { "ReadOne" },
        ))
    })
}

pub(super) fn query_error(stage: &str, error: zbus::Error) -> SlateError {
    let reason = match &error {
        zbus::Error::InputOutput(error) => format!("connection I/O error ({:?})", error.kind()),
        zbus::Error::Address(_) => "invalid session bus address".into(),
        zbus::Error::Handshake(_) => "connection handshake failed".into(),
        zbus::Error::InvalidReply | zbus::Error::Variant(_) => "invalid reply".into(),
        zbus::Error::MethodError(name, _, _)
            if name.as_str() == "org.freedesktop.DBus.Error.AccessDenied" =>
        {
            "access denied".into()
        }
        zbus::Error::FDO(error) if matches!(error.as_ref(), zbus::fdo::Error::AccessDenied(_)) => {
            "access denied".into()
        }
        _ => "request failed".into(),
    };
    SlateError::PlatformError(format!(
        "Portal Settings {stage} query failed: {reason}; response details omitted"
    ))
}

#[cfg(test)]
#[path = "settings_tests.rs"]
mod tests;
