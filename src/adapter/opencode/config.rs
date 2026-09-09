//! Lossless root-field edits: JSONC syntax is parsed, never stripped or
//! reserialized. Comments, whitespace and unrelated values keep their bytes.
use super::OpencodeAdapter;
use crate::config::file_read::{self, Links, Source, MAX_TOOL_CONFIG_BYTES};
use crate::config::state_files::atomic_write_synced_mode;
use crate::error::{Result, SlateError};
use jsonc_parser::ast::{Object, ObjectProp};
use jsonc_parser::common::Ranged;
use jsonc_parser::tokens::{Token, TokenAndRange};
use jsonc_parser::{parse_to_ast, CollectOptions, CommentCollectionStrategy, ParseOptions};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

#[cfg(test)]
mod tests;

fn invalid(path: &Path, reason: &str) -> SlateError {
    SlateError::ConfigReadError(path.display().to_string(), reason.to_owned())
}

fn source(path: &Path) -> Result<Option<Source>> {
    file_read::read(path, MAX_TOOL_CONFIG_BYTES, Links::Reject)
        .map_err(|error| invalid(path, &error.to_string()))
}

fn comment(token: &Token<'_>) -> bool {
    matches!(token, Token::CommentLine(_) | Token::CommentBlock(_))
}

pub(crate) struct Document<'a> {
    text: &'a str,
    root: Object<'a>,
    tokens: Vec<TokenAndRange<'a>>,
}

impl<'a> Document<'a> {
    pub(crate) fn parse(text: &'a str, path: &Path) -> Result<Self> {
        if text.len() as u64 > MAX_TOOL_CONFIG_BYTES {
            return Err(invalid(path, "OpenCode config exceeds the 8 MiB limit"));
        }
        // The library defaults include JSON5-like extensions. OpenCode's
        // JSONC contract here allows only comments and trailing commas.
        let parsed = parse_to_ast(
            text,
            &CollectOptions {
                comments: CommentCollectionStrategy::AsTokens,
                tokens: true,
            },
            &ParseOptions {
                allow_comments: true,
                allow_trailing_commas: true,
                allow_loose_object_property_names: false,
                allow_missing_commas: false,
                allow_single_quoted_strings: false,
                allow_hexadecimal_numbers: false,
                allow_unary_plus_numbers: false,
            },
        )
        .map_err(|_| invalid(path, "invalid OpenCode JSON/JSONC; file contents omitted"))?;
        let Some(jsonc_parser::ast::Value::Object(root)) = parsed.value else {
            return Err(invalid(path, "OpenCode TUI config must be a JSON object"));
        };
        let tokens = parsed.tokens.unwrap_or_default();
        // Even with JSON5 options disabled, the scanner accepts raw newlines
        // in strings. Validate literal syntax with serde, without converting
        // numbers (which would round or reject otherwise valid large values).
        if tokens.iter().any(|token| match token.token {
            Token::String(_) => serde_json::from_str::<String>(token.text(text)).is_err(),
            Token::Number(_) => {
                serde_json::from_str::<serde::de::IgnoredAny>(token.text(text)).is_err()
            }
            _ => false,
        }) {
            return Err(invalid(
                path,
                "invalid OpenCode JSON literal; file contents omitted",
            ));
        }
        let mut names = HashSet::new();
        for property in &root.properties {
            if !names.insert(property.name.as_str()) {
                return Err(invalid(
                    path,
                    "duplicate OpenCode root fields; inspect the config before editing",
                ));
            }
        }
        if root
            .properties
            .iter()
            .any(|p| p.name.as_str() == "theme" && p.value.as_string_lit().is_none())
        {
            return Err(invalid(
                path,
                "OpenCode theme must be a string; inspect the config before editing",
            ));
        }
        Ok(Self { text, root, tokens })
    }

    fn property(&self, name: &str) -> Option<&ObjectProp<'a>> {
        self.root
            .properties
            .iter()
            .find(|p| p.name.as_str() == name)
    }

    fn string(&self, name: &str) -> Option<&str> {
        self.property(name)?
            .value
            .as_string_lit()
            .map(|v| v.value.as_ref())
    }

    /// Inspect the theme without exposing user-supplied strings in diagnostics.
    /// None means unset; false means a custom theme, not a parse/type failure.
    pub(crate) fn has_system_theme(&self) -> Option<bool> {
        self.string("theme").map(|theme| theme == "system")
    }

    fn system(&self, path: &Path) -> Result<Option<Vec<u8>>> {
        if self.has_system_theme() == Some(true) {
            return Ok(None);
        }
        let mut edits = Vec::new();
        let mut additions = Vec::new();
        if let Some(theme) = self.property("theme") {
            edits.push((
                theme.value.start()..theme.value.end(),
                "\"system\"".to_owned(),
            ));
        } else {
            additions.push("\"theme\": \"system\"".to_owned());
        }
        if self.property("$schema").is_none() {
            additions.push(format!("\"$schema\": \"{}\"", OpencodeAdapter::TUI_SCHEMA));
        }
        if !additions.is_empty() {
            let newline = if self.text.contains("\r\n") {
                "\r\n"
            } else {
                "\n"
            };
            let multiline = self.text[self.root.start()..self.root.end()].contains('\n');
            let indent = self
                .root
                .properties
                .first()
                .and_then(|p| {
                    let prefix = self.text[..p.start()].rsplit_once('\n')?.1;
                    prefix
                        .bytes()
                        .all(|b| b == b' ' || b == b'\t')
                        .then_some(prefix)
                })
                .unwrap_or("  ");
            let separator = if multiline {
                format!(",{newline}{indent}")
            } else {
                ", ".to_owned()
            };
            // Insert immediately after a value, BEFORE its trailing comment or
            // comma. This cannot place a new field inside a line comment.
            let (at, prefix, suffix) = if let Some(last) = self.root.properties.last() {
                (last.value.end(), separator.clone(), String::new())
            } else if multiline {
                (
                    self.root.start() + 1,
                    format!("{newline}{indent}"),
                    newline.to_owned(),
                )
            } else {
                (self.root.start() + 1, String::new(), String::new())
            };
            edits.push((
                at..at,
                format!("{prefix}{}{suffix}", additions.join(&separator)),
            ));
        }
        let output = self.edited(edits);
        // Validate our own splice as well as the source before any writes.
        let checked = Document::parse(&output, path)?;
        if checked.string("theme") != Some("system") {
            return Err(invalid(
                path,
                "cannot prepare OpenCode theme edit; contents omitted",
            ));
        }
        drop(checked);
        Ok(Some(output.into_bytes()))
    }

    /// None means a theme-only, comment-free document may be removed. All
    /// comments are user data, even if they surround the removed theme field.
    pub(crate) fn without_system_theme(&self, path: &Path) -> Result<Option<Vec<u8>>> {
        if self.has_system_theme() != Some(true) {
            return Ok(Some(self.text.as_bytes().to_vec()));
        }
        let removable = self.root.properties.iter().all(|p| {
            p.name.as_str() == "theme"
                || (p.name.as_str() == "$schema"
                    && self.string("$schema") == Some(OpencodeAdapter::TUI_SCHEMA))
        });
        if removable && !self.tokens.iter().any(|t| comment(&t.token)) {
            return Ok(None);
        }
        let theme = self.property("theme").expect("system theme property");
        // Remove syntax tokens only, preserving comments even between the
        // property name, colon and value. Never touch nested theme properties.
        let mut edits: Vec<_> = self
            .tokens
            .iter()
            .filter(|t| t.start() >= theme.start() && t.end() <= theme.end() && !comment(&t.token))
            .map(|t| (t.start()..t.end(), String::new()))
            .collect();
        let following = self
            .tokens
            .iter()
            .find(|t| t.start() >= theme.end() && !comment(&t.token));
        let comma = following
            .filter(|t| matches!(t.token, Token::Comma))
            .or_else(|| {
                self.tokens
                    .iter()
                    .rev()
                    .find(|t| t.end() <= theme.start() && !comment(&t.token))
                    .filter(|t| matches!(t.token, Token::Comma))
            });
        if let Some(comma) = comma {
            edits.push((comma.start()..comma.end(), String::new()));
        }
        let output = self.edited(edits);
        Document::parse(&output, path)?;
        Ok(Some(output.into_bytes()))
    }

    fn edited(&self, mut edits: Vec<(std::ops::Range<usize>, String)>) -> String {
        // Reverse-order splices keep every original offset valid, including an
        // insertion immediately following a value that is also being replaced.
        edits.sort_by_key(|(range, _)| std::cmp::Reverse(range.start));
        let mut output = self.text.to_owned();
        for (range, replacement) in edits {
            output.replace_range(range, &replacement);
        }
        output
    }
}

pub(super) struct Prepared {
    path: PathBuf,
    destination: PathBuf,
    original: Option<Source>,
    replacement: Option<Vec<u8>>,
}

impl Prepared {
    pub(super) fn read(path: &Path) -> Result<Self> {
        let destination = file_read::directory_alias_target(path)
            .ok_or_else(|| invalid(path, "cannot resolve OpenCode config destination"))?;
        let original = source(path)?;
        let replacement = if let Some(original) = &original {
            let text = std::str::from_utf8(&original.bytes).map_err(|_| {
                invalid(path, "OpenCode config is not UTF-8; file contents omitted")
            })?;
            Document::parse(text, path)?.system(path)?
        } else {
            Document::parse("{\n}\n", path)?.system(path)?
        };
        let prepared = Self {
            path: path.to_owned(),
            destination,
            original,
            replacement,
        };
        prepared.verify()?;
        Ok(prepared)
    }

    pub(super) fn original_bytes(&self) -> Option<&[u8]> {
        self.original.as_ref().map(|source| source.bytes.as_slice())
    }
    pub(super) fn changed(&self) -> bool {
        self.replacement.is_some()
    }

    pub(super) fn verify(&self) -> Result<()> {
        // Source identity alone cannot detect a moved directory alias when
        // both targets are missing (or refer to the same hard-linked inode).
        if file_read::directory_alias_target(&self.path).as_ref() != Some(&self.destination) {
            return Err(invalid(
                &self.path,
                "OpenCode config destination changed while preparing; review the path before retrying",
            ));
        }
        if source(&self.path)? != self.original {
            return Err(invalid(
                &self.path,
                "OpenCode config changed while preparing; retry without overwriting the edit",
            ));
        }
        Ok(())
    }

    pub(super) fn publish(self) -> Result<()> {
        self.verify()?;
        if let Some(bytes) = &self.replacement {
            if let Some(parent) = self.path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            // Best-effort conflict detection, not a lock on external editors.
            self.verify()?;
            atomic_write_synced_mode(
                &self.path,
                bytes,
                self.original.as_ref().and_then(|source| source.mode),
            )?;
        }
        Ok(())
    }
}
