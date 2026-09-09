use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// Transport context captured once, without querying or changing running apps.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SessionContext {
    remote: bool,
    multiplexed: bool,
    isolated: bool,
    tmux_socket: Option<PathBuf>,
}

impl SessionContext {
    pub fn from_process() -> Self {
        Self::from_vars(|key| std::env::var_os(key))
    }

    pub fn from_vars(vars: impl Fn(&str) -> Option<OsString>) -> Self {
        let present = |key| vars(key).is_some_and(|value| !value.is_empty());
        let tmux = vars("TMUX").filter(|value| !value.is_empty());
        Self {
            remote: present("SSH_CONNECTION") || present("SSH_TTY") || present("SSH_CLIENT"),
            multiplexed: tmux.is_some(),
            isolated: present("SLATE_HOME"),
            tmux_socket: tmux
                .as_ref()
                .and_then(|value| value.to_str())
                .and_then(parse_tmux_socket),
        }
    }

    pub fn isolated() -> Self {
        Self {
            isolated: true,
            ..Self::default()
        }
    }

    pub fn is_remote(&self) -> bool {
        self.remote
    }
    pub fn is_multiplexed(&self) -> bool {
        self.multiplexed
    }
    pub fn is_isolated(&self) -> bool {
        self.isolated
    }
    pub fn tmux_socket(&self) -> Option<&Path> {
        self.tmux_socket.as_deref()
    }
    pub fn can_reload_terminal(&self) -> bool {
        !self.remote && !self.isolated
    }
}

fn parse_tmux_socket(value: &str) -> Option<PathBuf> {
    // Socket paths can contain commas; the two numeric suffixes cannot.
    let mut fields = value.rsplitn(3, ',');
    fields.next()?.parse::<u32>().ok()?;
    if fields.next()?.parse::<u32>().ok()? == 0 {
        return None;
    }
    let path = PathBuf::from(fields.next()?);
    path.is_absolute().then_some(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_context_keeps_remote_and_tmux_distinct() {
        let session = SessionContext::from_vars(|key| match key {
            "TMUX" => Some("/tmp/slate,work/socket,123,0".into()),
            "SSH_CONNECTION" => Some("remote connection".into()),
            _ => None,
        });
        assert!(session.is_remote() && session.is_multiplexed());
        assert_eq!(
            session.tmux_socket(),
            Some(Path::new("/tmp/slate,work/socket"))
        );
        assert!(!session.can_reload_terminal());
        for invalid in ["", "relative,123,0", "/tmp/socket,0,0", "/tmp/socket,abc,0"] {
            assert!(parse_tmux_socket(invalid).is_none());
        }
        assert!(!SessionContext::isolated().can_reload_terminal());
        assert!(SessionContext::default().can_reload_terminal());
    }
}
