//! Cancellable Portal Settings subscription. Own the connection through setup,
//! signal handling and shutdown; idle watching has no short-query deadline.
use super::{
    query_deadline::QUERY_TIMEOUT,
    settings::{APPEARANCE_NAMESPACE, COLOR_SCHEME_KEY},
};
use crate::error::{Result, SlateError};
use futures_lite::{future, StreamExt};
use std::{future::Future, time::Duration};
use zbus::{
    names::OwnedUniqueName, zvariant::Value, Connection, MatchRule, Message, MessageStream,
};

const SERVICE: &str = "org.freedesktop.portal.Desktop";
const PATH: &str = "/org/freedesktop/portal/desktop";
const INTERFACE: &str = "org.freedesktop.portal.Settings";
const QUEUED_SIGNALS: usize = 16;

fn error(message: &str) -> SlateError {
    SlateError::PlatformError(format!(
        "Portal appearance watcher {message}; response details omitted"
    ))
}

fn stage_error(stage: &str, source: zbus::Error) -> SlateError {
    // Reuse the finite-query error categories without exposing any wire body.
    super::settings::query_error(stage, source)
}

pub(crate) async fn run(
    connect: impl Future<Output = zbus::Result<Connection>>,
    cancel: impl Future<Output = ()>,
    ready: impl FnOnce() -> Result<()>,
    mut on_change: impl FnMut(u32) -> Result<()>,
) -> Result<()> {
    let mut connection = None;
    let result = future::or(
        async {
            cancel.await;
            Ok(())
        },
        async {
            let subscription = future::or(
                async {
                    async_io::Timer::after(QUERY_TIMEOUT).await;
                    Err(error(&format!(
                        "startup timed out after {} ms",
                        QUERY_TIMEOUT.as_millis()
                    )))
                },
                async {
                    connection = Some(
                        connect
                            .await
                            .map_err(|e| stage_error("watch connection", e))?,
                    );
                    Subscription::new(connection.as_ref().unwrap()).await
                },
            )
            .await?;
            ready()?;
            subscription.consume(&mut on_change).await
        },
    )
    .await;

    // Closing the exclusively owned transport also ends zbus's reader and any
    // queued match-rule removal. Do not leave a detached blocking worker behind.
    if let Some(connection) = connection {
        let close = future::or(
            async {
                async_io::Timer::after(Duration::from_secs(1)).await;
                Err(error("connection shutdown timed out"))
            },
            async {
                connection
                    .close()
                    .await
                    .map_err(|e| stage_error("watch shutdown", e))
            },
        )
        .await;
        if result.is_ok() {
            close?;
        }
    }
    result
}

struct Subscription {
    changes: MessageStream,
    owners: MessageStream,
    owner: OwnedUniqueName,
}

impl Subscription {
    async fn new(connection: &Connection) -> Result<Self> {
        // Register owner changes before resolving the owner, so disappearance
        // during setup cannot leave a permanently dormant, supposedly ready source.
        let owner_rule = MatchRule::builder()
            .msg_type(zbus::message::Type::Signal)
            .sender("org.freedesktop.DBus")
            .unwrap()
            .path("/org/freedesktop/DBus")
            .unwrap()
            .interface("org.freedesktop.DBus")
            .unwrap()
            .member("NameOwnerChanged")
            .unwrap()
            .add_arg(SERVICE)
            .unwrap()
            .build();
        let mut owners =
            MessageStream::for_match_rule(owner_rule, connection, Some(QUEUED_SIGNALS))
                .await
                .map_err(|e| stage_error("watch owner subscription", e))?;
        let bus = zbus::fdo::DBusProxy::builder(connection)
            .cache_properties(zbus::proxy::CacheProperties::No)
            .build()
            .await
            .map_err(|e| stage_error("watch bus proxy", e))?;
        let owner = bus
            .get_name_owner(SERVICE.try_into().unwrap())
            .await
            .map_err(|e| stage_error("watch service owner", e.into()))?;
        let change_rule = MatchRule::builder()
            .msg_type(zbus::message::Type::Signal)
            .sender(owner.as_str())
            .unwrap()
            .path(PATH)
            .unwrap()
            .interface(INTERFACE)
            .unwrap()
            .member("SettingChanged")
            .unwrap()
            .build();
        let changes = MessageStream::for_match_rule(change_rule, connection, Some(QUEUED_SIGNALS))
            .await
            .map_err(|e| stage_error("watch settings subscription", e))?;
        while let Some(message) = future::poll_once(owners.next()).await {
            check_owner(next_message(message, "service-owner stream")?, &owner)?;
            future::yield_now().await;
        }
        Ok(Self {
            changes,
            owners,
            owner,
        })
    }

    async fn consume(mut self, on_change: &mut impl FnMut(u32) -> Result<()>) -> Result<()> {
        loop {
            let (owner_event, message) =
                future::or(async { (true, self.owners.next().await) }, async {
                    (false, self.changes.next().await)
                })
                .await;
            let message = next_message(
                message,
                if owner_event {
                    "service-owner stream"
                } else {
                    "settings stream"
                },
            )?;
            if owner_event {
                check_owner(message, &self.owner)?;
            } else if let Some(value) = decode_change(&message)? {
                on_change(value)?;
            }
            // A continuously ready signal stream must not starve cancellation.
            future::yield_now().await;
        }
    }
}

fn next_message(message: Option<zbus::Result<Message>>, stream: &str) -> Result<Message> {
    message
        .ok_or_else(|| error(&format!("{stream} closed")))?
        .map_err(|e| stage_error("watch signal stream", e))
}

fn check_owner(message: Message, owner: &OwnedUniqueName) -> Result<()> {
    // A well-known sender in a raw match rule is not locally authenticated by
    // zbus. The bus daemon uses its reserved sender; ignore injected unicast.
    if message.header().sender().map(|s| s.as_str()) != Some("org.freedesktop.DBus") {
        return Ok(());
    }
    let body = message.body();
    let (name, _old, new): (&str, &str, &str) = body
        .deserialize()
        .map_err(|_| error("received an invalid service-owner notification"))?;
    if name == SERVICE && new != owner.as_str() {
        return Err(error(
            "service owner disappeared or changed; restart the watcher",
        ));
    }
    Ok(())
}

fn decode_change(message: &Message) -> Result<Option<u32>> {
    let body = message.body();
    let (namespace, key, value): (&str, &str, Value<'_>) = body
        .deserialize()
        .map_err(|_| error("received an invalid Settings notification"))?;
    if namespace != APPEARANCE_NAMESPACE || key != COLOR_SCHEME_KEY {
        return Ok(None);
    }
    match value {
        Value::U32(value) => Ok(Some(value)),
        _ => Err(error("received an invalid color-scheme type")),
    }
}

#[cfg(test)]
#[path = "watch_tests.rs"]
mod tests;
