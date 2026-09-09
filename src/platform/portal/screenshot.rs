//! Subscribe before invoking Screenshot, including old portals that return a
//! different request path. Each operation exclusively owns its bus connection.
use super::{
    interfaces::PortalScreenshotProxy, query_deadline::QUERY_TIMEOUT, PortalCaptureStatus,
};
use crate::error::{Result, SlateError};
use futures_lite::{future, StreamExt};
use std::{collections::HashMap, future::Future, path::Path, time::Duration};
use zbus::{
    names::OwnedUniqueName,
    zvariant::{OwnedObjectPath, OwnedValue, Value},
    Connection, MatchRule, Message, MessageStream,
};

const SERVICE: &str = "org.freedesktop.portal.Desktop";
const RESPONSE_INTERFACE: &str = "org.freedesktop.portal.Request";
const MAX_PENDING: usize = 16;
const MAX_RESPONSE_BYTES: usize = 64 * 1024;

fn error(reason: &str) -> SlateError {
    SlateError::PlatformError(format!(
        "Portal screenshot {reason}; response details omitted"
    ))
}

fn wire_error(stage: &str, source: zbus::Error) -> SlateError {
    let category = match source {
        zbus::Error::FDO(e) if matches!(*e, zbus::fdo::Error::AccessDenied(_)) => "access denied",
        zbus::Error::MethodError(name, _, _)
            if name.as_str() == "org.freedesktop.DBus.Error.AccessDenied" =>
        {
            "access denied"
        }
        zbus::Error::Variant(_) | zbus::Error::InvalidReply => "invalid reply",
        zbus::Error::InputOutput(_) => "connection I/O error",
        _ => "request failed",
    };
    error(&format!("{stage}: {category}"))
}

async fn deadline<T>(
    work: impl Future<Output = Result<T>>,
    duration: Duration,
    stage: &str,
) -> Result<T> {
    future::or(
        async {
            async_io::Timer::after(duration).await;
            Err(error(&format!(
                "{stage} timed out after {} ms",
                duration.as_millis()
            )))
        },
        work,
    )
    .await
}

async fn finish<T>(connection: Option<Connection>, result: Result<T>) -> Result<T> {
    if let Some(connection) = connection {
        // Explicitly end zbus's reader and queued match removals, even after a
        // setup error. No detached blocking worker is used for the deadline.
        let closed = deadline(
            async {
                connection
                    .close()
                    .await
                    .map_err(|e| wire_error("shutdown", e))
            },
            Duration::from_secs(1),
            "shutdown",
        )
        .await;
        if result.is_ok() && closed.is_err() {
            // A consumer may already have published the image. A cleanup error
            // must not misreport that completed operation as a failed capture.
            eprintln!("warning: Portal screenshot operation completed, but its connection could not be cleanly closed");
        }
    }
    result
}

#[cfg(target_os = "linux")]
pub(super) fn version() -> Result<u32> {
    async_io::block_on(version_query(Connection::session(), QUERY_TIMEOUT))
}

async fn version_query(
    connect: impl Future<Output = zbus::Result<Connection>>,
    timeout: Duration,
) -> Result<u32> {
    let mut connection = None;
    let result = deadline(
        async {
            connection = Some(connect.await.map_err(|e| wire_error("connection", e))?);
            let proxy = PortalScreenshotProxy::builder(connection.as_ref().unwrap())
                .cache_properties(zbus::proxy::CacheProperties::No)
                .build()
                .await
                .map_err(|e| wire_error("proxy", e))?;
            proxy.version().await.map_err(|e| wire_error("version", e))
        },
        timeout,
        "version query",
    )
    .await;
    finish(connection, result).await
}

#[cfg(target_os = "linux")]
pub(super) fn capture(output: &Path) -> Result<PortalCaptureStatus> {
    async_io::block_on(capture_with(Connection::session(), output, QUERY_TIMEOUT))
}

async fn capture_with(
    connect: impl Future<Output = zbus::Result<Connection>>,
    output: &Path,
    timeout: Duration,
) -> Result<PortalCaptureStatus> {
    match std::fs::symlink_metadata(output) {
        Ok(_) => return Err(error("output already exists; choose an unused path")),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e.into()),
    }
    request_with(connect, timeout, |response| match response {
        Response::Uri(uri) => {
            // Keep the requesting connection alive while consuming borrowed
            // access; don't assume URI permissions survive bus disconnection.
            super::screenshot_file::copy_result(&uri, output)?;
            Ok(PortalCaptureStatus::Captured)
        }
        Response::Cancelled => Ok(PortalCaptureStatus::Cancelled),
    })
    .await
}

#[derive(Debug, PartialEq, Eq)]
enum Response {
    Uri(String),
    Cancelled,
}

async fn request_with<T>(
    connect: impl Future<Output = zbus::Result<Connection>>,
    timeout: Duration,
    consume: impl FnOnce(Response) -> Result<T>,
) -> Result<T> {
    let mut connection = None;
    let result = async {
        let subscription = deadline(
            async {
                connection = Some(connect.await.map_err(|e| wire_error("connection", e))?);
                Subscription::new(connection.as_ref().unwrap()).await
            },
            timeout,
            "setup",
        )
        .await?;
        // The short deadline ends before the user-facing method is invoked.
        // Waiting for its reply or the user's selection has no idle timeout.
        let response = subscription.interact(connection.as_ref().unwrap()).await?;
        consume(response)
    }
    .await;
    finish(connection, result).await
}

struct Subscription {
    responses: MessageStream,
    owners: MessageStream,
    owner: OwnedUniqueName,
    version: u32,
}

impl Subscription {
    async fn new(connection: &Connection) -> Result<Self> {
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
        let mut owners = MessageStream::for_match_rule(owner_rule, connection, Some(MAX_PENDING))
            .await
            .map_err(|e| wire_error("owner subscription", e))?;
        let bus = zbus::fdo::DBusProxy::builder(connection)
            .cache_properties(zbus::proxy::CacheProperties::No)
            .build()
            .await
            .map_err(|e| wire_error("bus proxy", e))?;
        let service = SERVICE.try_into().unwrap();
        let owner = match bus.get_name_owner(service).await {
            Ok(owner) => owner,
            Err(zbus::fdo::Error::NameHasNoOwner(_)) => {
                bus.start_service_by_name(SERVICE.try_into().unwrap(), 0)
                    .await
                    .map_err(|e| wire_error("service activation", e.into()))?;
                bus.get_name_owner(SERVICE.try_into().unwrap())
                    .await
                    .map_err(|e| wire_error("service owner", e.into()))?
            }
            Err(e) => return Err(wire_error("service owner", e.into())),
        };
        let proxy = screenshot_proxy(connection, &owner).await?;
        let version = proxy
            .version()
            .await
            .map_err(|e| wire_error("version", e))?;
        // Temporarily match every Request path from this unique owner on our
        // private connection. This catches even a legacy handle's early Response;
        // accepting a result still requires the exact path returned by our call.
        let response_rule = MatchRule::builder()
            .msg_type(zbus::message::Type::Signal)
            .sender(owner.as_str())
            .unwrap()
            .interface(RESPONSE_INTERFACE)
            .unwrap()
            .member("Response")
            .unwrap()
            .build();
        let responses = MessageStream::for_match_rule(response_rule, connection, Some(MAX_PENDING))
            .await
            .map_err(|e| wire_error("response subscription", e))?;
        while let Some(message) = future::poll_once(owners.next()).await {
            check_owner(next(message)?, &owner)?;
            future::yield_now().await;
        }
        Ok(Self {
            responses,
            owners,
            owner,
            version,
        })
    }

    async fn interact(mut self, connection: &Connection) -> Result<Response> {
        future::or(
            async {
                loop {
                    check_owner(next(self.owners.next().await)?, &self.owner)?;
                    future::yield_now().await;
                }
            },
            perform(connection, &self.owner, self.version, &mut self.responses),
        )
        .await
    }
}

async fn screenshot_proxy<'a>(
    connection: &Connection,
    owner: &'a OwnedUniqueName,
) -> Result<PortalScreenshotProxy<'a>> {
    PortalScreenshotProxy::builder(connection)
        .destination(owner.as_str())
        .map_err(|e| wire_error("destination", e))?
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await
        .map_err(|e| wire_error("proxy", e))
}

fn handle_token() -> Result<String> {
    let mut random = [0u8; 32];
    getrandom::fill(&mut random).map_err(|_| error("could not obtain request-token entropy"))?;
    use std::fmt::Write;
    let mut token = String::from("slate_shot_");
    for byte in random {
        write!(&mut token, "{byte:02x}").unwrap();
    }
    Ok(token)
}

async fn perform(
    connection: &Connection,
    owner: &OwnedUniqueName,
    version: u32,
    responses: &mut MessageStream,
) -> Result<Response> {
    let proxy = screenshot_proxy(connection, owner).await?;
    let token = handle_token()?;
    let mut options = HashMap::new();
    options.insert(
        "handle_token",
        Value::from(token)
            .try_to_owned()
            .map_err(|e| wire_error("token", e.into()))?,
    );
    if version >= 2 {
        options.insert("interactive", OwnedValue::from(true));
    }
    let call = proxy.screenshot("", options);
    futures_lite::pin!(call);
    let mut pending = Vec::new();
    enum Event {
        Handle(zbus::Result<OwnedObjectPath>),
        Signal(Option<zbus::Result<Message>>),
    }
    let handle = loop {
        match future::or(async { Event::Handle(call.as_mut().await) }, async {
            Event::Signal(responses.next().await)
        })
        .await
        {
            Event::Handle(handle) => break handle.map_err(|e| wire_error("request", e))?,
            Event::Signal(message) => {
                let message = next(message)?;
                if message.header().sender().map(|s| s.as_str()) == Some(owner.as_str()) {
                    // Drain while the method reply is pending: an unpolled full
                    // zbus signal queue can otherwise stall receipt of that reply.
                    if pending.len() == MAX_PENDING || message.body().len() > MAX_RESPONSE_BYTES {
                        return Err(error("early responses exceeded the buffering limit"));
                    }
                    pending.push(message);
                }
            }
        }
        future::yield_now().await;
    };
    for message in pending {
        if matches_handle(&message, owner, &handle) {
            return decode(&message);
        }
    }
    loop {
        let message = next(responses.next().await)?;
        if matches_handle(&message, owner, &handle) {
            return decode(&message);
        }
        future::yield_now().await;
    }
}

fn matches_handle(message: &Message, owner: &OwnedUniqueName, handle: &OwnedObjectPath) -> bool {
    let header = message.header();
    header.sender().map(|s| s.as_str()) == Some(owner.as_str())
        && header.path().map(|p| p.as_str()) == Some(handle.as_str())
}

fn next(message: Option<zbus::Result<Message>>) -> Result<Message> {
    message
        .ok_or_else(|| error("connection or response stream closed"))?
        .map_err(|e| wire_error("signal stream", e))
}

fn check_owner(message: Message, owner: &OwnedUniqueName) -> Result<()> {
    if message.header().sender().map(|s| s.as_str()) != Some("org.freedesktop.DBus") {
        return Ok(());
    }
    let body = message.body();
    let (name, _old, new): (&str, &str, &str) = body
        .deserialize()
        .map_err(|_| error("received an invalid service-owner notification"))?;
    if name == SERVICE && new != owner.as_str() {
        return Err(error(
            "service owner disappeared or changed; retry the screenshot",
        ));
    }
    Ok(())
}

fn decode(message: &Message) -> Result<Response> {
    let body = message.body();
    if body.len() > MAX_RESPONSE_BYTES {
        return Err(error("response exceeded the size limit"));
    }
    let (status, results): (u32, HashMap<&str, Value<'_>>) = body
        .deserialize()
        .map_err(|_| error("received an invalid response"))?;
    match status {
        0 => match results.get("uri") {
            Some(Value::Str(uri)) => Ok(Response::Uri(uri.to_string())),
            _ => Err(error("response did not include a string URI")),
        },
        1 => Ok(Response::Cancelled),
        code => Err(error(&format!("request ended with response code {code}"))),
    }
}

#[cfg(test)]
#[path = "screenshot_tests.rs"]
mod tests;
