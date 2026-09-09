use crate::platform::capabilities::CapabilityReport;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShellBackend {
    Zsh,
    Bash,
    Fish,
    Unsupported,
}

impl ShellBackend {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Zsh => "zsh",
            Self::Bash => "bash",
            Self::Fish => "fish",
            Self::Unsupported => "unsupported",
        }
    }
}

pub fn detect_backend() -> ShellBackend {
    detect_backend_from_shell(std::env::var("SHELL").ok().as_deref())
}

/// A literal UTF-8 word for Fish source. Unlike POSIX single quotes, Fish
/// interprets escaped backslashes and apostrophes inside single quotes.
/// https://fishshell.com/docs/current/language.html#quotes
pub(crate) fn fish_quote(value: &str) -> String {
    let mut quoted = String::with_capacity(value.len() + 2);
    quoted.push('\'');
    for character in value.chars() {
        if matches!(character, '\\' | '\'') {
            quoted.push('\\');
        }
        quoted.push(character);
    }
    quoted.push('\'');
    quoted
}

pub fn detect_backend_from_shell(shell: Option<&str>) -> ShellBackend {
    let Some(shell) = shell else {
        return ShellBackend::Unsupported;
    };

    let shell = shell.trim();
    if shell.is_empty() {
        return ShellBackend::Unsupported;
    }

    let name = Path::new(shell)
        .file_name()
        .and_then(|part| part.to_str())
        .unwrap_or(shell)
        .trim()
        .to_ascii_lowercase();

    match name.as_str() {
        "zsh" => ShellBackend::Zsh,
        "bash" => ShellBackend::Bash,
        "fish" => ShellBackend::Fish,
        _ => ShellBackend::Unsupported,
    }
}

pub fn capability_report() -> CapabilityReport {
    match detect_backend() {
        ShellBackend::Zsh => CapabilityReport::supported("zsh"),
        ShellBackend::Bash => CapabilityReport::supported("bash"),
        ShellBackend::Fish => CapabilityReport::supported("fish"),
        ShellBackend::Unsupported => CapabilityReport::unsupported(
            "unsupported",
            "Slate shell integration currently targets zsh, bash, and fish only.",
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fish_paths_quote_literals_without_posix_backslash_assumptions() {
        for (input, expected) in [
            ("", "''"),
            ("plain space 中文", "'plain space 中文'"),
            (r"two\\slashes", r"'two\\\\slashes'"),
            (r"trailing\", r"'trailing\\'"),
            (r"slash\'quote", r"'slash\\\'quote'"),
            ("a'b", r"'a\'b'"),
            (
                "$HOME (literal) $(literal) `literal`;*?#[]",
                "'$HOME (literal) $(literal) `literal`;*?#[]'",
            ),
            ("line\nnext\tcolumn", "'line\nnext\tcolumn'"),
        ] {
            assert_eq!(fish_quote(input), expected, "{input:?}");
        }
    }

    #[test]
    fn test_detect_backend_from_shell_recognizes_supported_shells() {
        assert_eq!(
            detect_backend_from_shell(Some("/bin/zsh")),
            ShellBackend::Zsh
        );
        assert_eq!(
            detect_backend_from_shell(Some("/usr/bin/bash")),
            ShellBackend::Bash
        );
        assert_eq!(
            detect_backend_from_shell(Some("/opt/homebrew/bin/fish")),
            ShellBackend::Fish
        );
    }

    #[test]
    fn test_detect_backend_from_shell_rejects_unknown_shells() {
        assert_eq!(detect_backend_from_shell(None), ShellBackend::Unsupported);
        assert_eq!(
            detect_backend_from_shell(Some("")),
            ShellBackend::Unsupported
        );
        assert_eq!(
            detect_backend_from_shell(Some("/Applications/WezTerm.app")),
            ShellBackend::Unsupported
        );
    }

    #[test]
    fn test_capability_report_uses_unsupported_label() {
        let report = CapabilityReport::unsupported(
            ShellBackend::Unsupported.label(),
            "Slate shell integration currently targets zsh, bash, and fish only.",
        );

        assert_eq!(report.backend, "unsupported");
        assert!(report.reason.is_some());
    }
}
