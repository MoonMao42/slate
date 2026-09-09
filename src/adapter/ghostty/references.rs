//! Literal references follow Ghostty 1.3.1's LineIterator then Path.parse.
//! This is deliberately not shell parsing (no escapes or inline comments).
use std::io;
use std::path::{Component, Path, PathBuf};

#[derive(Clone)]
pub(crate) struct Reference {
    pub path: PathBuf,
    pub optional: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Directive<'a> {
    Ignore,
    Reset,
    Invalid,
    Path {
        value: &'a str,
        optional: bool,
        legacy: bool,
    },
}

fn unquote(value: &str) -> &str {
    if value.len() >= 2 && value.starts_with('"') && value.ends_with('"') {
        &value[1..value.len() - 1]
    } else {
        value
    }
}

pub(crate) fn parse(line: &str) -> Directive<'_> {
    let line = line.trim_matches([' ', '\t', '\r']);
    if line.is_empty() || line.starts_with('#') {
        return Directive::Ignore;
    }
    let Some((key, value)) = line.split_once('=') else {
        return if line.split([' ', '\t']).next() == Some("config-file") {
            Directive::Invalid
        } else {
            Directive::Ignore
        };
    };
    let key = key.trim_matches([' ', '\t']);
    if key != "config-file" && key != "include" {
        return Directive::Ignore;
    }
    let legacy = key == "include";
    // First quote pair is stripped by the config line reader, before Path.parse
    // sees the optional marker. Path.parse may then strip a second quote pair.
    let value = unquote(value.trim_matches([' ', '\t']));
    if value.is_empty() {
        return if legacy {
            Directive::Ignore
        } else {
            Directive::Reset
        };
    }
    let (value, optional) = value
        .strip_prefix('?')
        .map_or((value, false), |v| (v, true));
    let value = unquote(value);
    if value.is_empty() {
        return Directive::Ignore;
    }
    if value.contains('\0') {
        return Directive::Invalid;
    }
    Directive::Path {
        value,
        optional,
        legacy,
    }
}

pub(crate) fn resolve(value: &str, parent: &Path, home: &Path) -> io::Result<PathBuf> {
    let path = Path::new(value);
    // Native absolute/~/ paths retain their spelling, including file symlinks.
    // Their nested references therefore use the containing link's directory.
    if path.is_absolute() {
        return Ok(path.to_owned());
    }
    if let Some(rest) = value.strip_prefix("~/") {
        return Ok(home.join(rest));
    }
    let joined = parent.join(path);
    match std::fs::canonicalize(&joined) {
        Ok(path) => Ok(path),
        // Ghostty uses lexical resolution only for absent relative paths, not
        // permission errors, non-directory prefixes or symlink loops.
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(lexical(&joined)),
        Err(error) => Err(error),
    }
}

fn lexical(path: &Path) -> PathBuf {
    let mut result = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                result.pop();
            }
            component => result.push(component.as_os_str()),
        }
    }
    result
}

pub(crate) fn managed(value: &str, root: &Path) -> Option<String> {
    let path = Path::new(value);
    // Literal ownership evidence only; do not turn /managed-old, ../, or an
    // arbitrary embedded string into a Slate-owned configuration reference.
    (path.is_absolute()
        && path.starts_with(root)
        && !path
            .components()
            .any(|part| matches!(part, Component::ParentDir)))
    .then(|| value.to_owned())
}

/// Preserve physical bytes while using the same literal reference grammar as
/// diagnostics. Strip a BOM only at the beginning of the file. Invalid UTF-8
/// lines remain opaque; this is not whole-file syntax/line-limit validation.
pub(crate) fn literal_lines(content: &[u8]) -> impl Iterator<Item = (&[u8], Directive<'_>)> {
    content
        .split_inclusive(|b| *b == b'\n')
        .enumerate()
        .map(|(index, raw)| {
            let line = raw.strip_suffix(b"\n").unwrap_or(raw);
            let line = if index == 0 {
                line.strip_prefix(b"\xef\xbb\xbf").unwrap_or(line)
            } else {
                line
            };
            let directive = std::str::from_utf8(line)
                .map(parse)
                .unwrap_or(Directive::Invalid);
            (raw, directive)
        })
}

/// Literal direct-reference evidence only. A local reset cancels preceding
/// references; legacy `include` hints do not represent Ghostty config-file edges.
pub(crate) fn contains_literal_reference(content: &[u8], target: &Path) -> bool {
    let Some(target) = target.to_str() else {
        return false;
    };
    let mut found = false;
    for (_, directive) in literal_lines(content) {
        match directive {
            Directive::Reset => found = false,
            Directive::Path {
                value,
                legacy: false,
                ..
            } if value == target => found = true,
            _ => {}
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ghostty_references_parse_full_literal_values_and_two_quote_layers() {
        for (line, value, optional) in [
            (
                "config-file = path with spaces.conf",
                "path with spaces.conf",
                false,
            ),
            (
                "config-file = \" path #= file \\\"inside\\\" \"",
                " path #= file \\\"inside\\\" ",
                false,
            ),
            (
                "config-file = name # not a comment",
                "name # not a comment",
                false,
            ),
            ("config-file = 'single quoted'", "'single quoted'", false),
            ("config-file = ?\"optional path\"", "optional path", true),
            ("config-file = \"?optional path\"", "optional path", true),
            (
                "config-file = \"\"?literal path\"\"",
                "?literal path",
                false,
            ),
            ("config-file = \"unterminated", "\"unterminated", false),
            ("\tconfig-file = ~/child.conf\r", "~/child.conf", false),
            ("config-file = ~", "~", false),
        ] {
            assert_eq!(
                parse(line),
                Directive::Path {
                    value,
                    optional,
                    legacy: false
                },
                "{line}"
            );
        }
        for line in ["config-file =", "config-file = \"\""] {
            assert_eq!(parse(line), Directive::Reset);
        }
        for line in [
            "# config-file = a",
            "theme = a",
            "config-file = ?",
            "config-file = ?\"\"",
        ] {
            assert_eq!(parse(line), Directive::Ignore);
        }
        for line in ["config-file", "config-file path", "config-file = a\0b"] {
            assert_eq!(parse(line), Directive::Invalid);
        }
        assert_eq!(
            parse("include = /tmp/old hook"),
            Directive::Path {
                value: "/tmp/old hook",
                optional: false,
                legacy: true
            }
        );
    }

    #[test]
    fn ghostty_references_resolve_absolute_and_relative_links_with_distinct_bases() {
        let td = tempfile::tempdir().unwrap();
        let dir = td.path().join("entry");
        let actual = td.path().join("actual");
        std::fs::create_dir(&dir).unwrap();
        std::fs::create_dir(&actual).unwrap();
        std::fs::write(actual.join("file"), "").unwrap();
        std::os::unix::fs::symlink(actual.join("file"), dir.join("link")).unwrap();
        let absolute = dir.join("link");
        assert_eq!(
            resolve(absolute.to_str().unwrap(), &dir, td.path()).unwrap(),
            absolute
        );
        assert_eq!(
            resolve("link", &dir, td.path()).unwrap(),
            actual.join("file").canonicalize().unwrap()
        );
        assert_eq!(
            resolve("~/name", &dir, td.path()).unwrap(),
            td.path().join("name")
        );
        assert_eq!(resolve("~", &dir, td.path()).unwrap(), dir.join("~"));
        assert!(resolve("link/child", &dir, td.path()).is_err());
    }

    #[test]
    fn ghostty_references_managed_evidence_uses_complete_path_components() {
        let root = Path::new("/home/space user/.config/slate/managed/ghostty");
        let path = root.join("theme with #= spaces.conf");
        assert_eq!(
            managed(path.to_str().unwrap(), root).as_deref(),
            path.to_str()
        );
        for value in [
            format!("{}-old/theme.conf", root.display()),
            format!("/other{}/theme.conf", root.display()),
            format!("{}/../other.conf", root.display()),
        ] {
            assert!(managed(&value, root).is_none());
        }
    }
}
