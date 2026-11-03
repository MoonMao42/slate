//! Lossless theme-name edits: JSONC syntax is parsed, never stripped or
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
            return Err(invalid(path, "OpenCode CLI config must be a JSON object"));
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
        let unique = |object: &Object<'_>| -> Result<()> {
            let mut names = HashSet::new();
            if object
                .properties
                .iter()
                .any(|p| !names.insert(p.name.as_str()))
            {
                return Err(invalid(
                    path,
                    "duplicate OpenCode config fields; inspect the config before editing",
                ));
            }
            Ok(())
        };
        unique(&root)?;
        if let Some(theme) = root.properties.iter().find(|p| p.name.as_str() == "theme") {
            let jsonc_parser::ast::Value::Object(theme) = &theme.value else {
                return Err(invalid(path, "OpenCode 2 theme must be an object"));
            };
            unique(theme)?;
            if theme
                .properties
                .iter()
                .any(|p| p.name.as_str() == "name" && p.value.as_string_lit().is_none())
            {
                return Err(invalid(path, "OpenCode theme.name must be a string"));
            }
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

    fn theme(&self) -> Option<&Object<'a>> {
        match &self.property("theme")?.value {
            jsonc_parser::ast::Value::Object(theme) => Some(theme),
            _ => None,
        }
    }

    fn theme_name(&self) -> Option<&ObjectProp<'a>> {
        self.theme()?
            .properties
            .iter()
            .find(|p| p.name.as_str() == "name")
    }

    /// None means unset; false means a custom theme, not a parse/type failure.
    pub(crate) fn has_slate_theme(&self) -> Option<bool> {
        self.theme_name()?
            .value
            .as_string_lit()
            .map(|name| name.value == "slate-sync")
    }

    fn insertion(
        &self,
        object: &Object<'_>,
        additions: &[String],
    ) -> (std::ops::Range<usize>, String) {
        let newline = if self.text.contains("\r\n") {
            "\r\n"
        } else {
            "\n"
        };
        let multiline = self.text[object.start()..object.end()].contains('\n');
        let indent = object
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
        // Insert before any trailing comment or comma.
        let (at, prefix, suffix) = if let Some(last) = object.properties.last() {
            (last.value.end(), separator.clone(), String::new())
        } else if multiline {
            (
                object.start() + 1,
                format!("{newline}{indent}"),
                newline.to_owned(),
            )
        } else {
            (object.start() + 1, String::new(), String::new())
        };
        (
            at..at,
            format!("{prefix}{}{suffix}", additions.join(&separator)),
        )
    }

    fn slate(&self, path: &Path) -> Result<Option<Vec<u8>>> {
        if self.has_slate_theme() == Some(true) {
            return Ok(None);
        }
        let mut edits = Vec::new();
        let mut additions = Vec::new();
        if let Some(name) = self.theme_name() {
            edits.push((
                name.value.start()..name.value.end(),
                "\"slate-sync\"".to_owned(),
            ));
        } else if let Some(theme) = self.theme() {
            edits.push(self.insertion(theme, &["\"name\": \"slate-sync\"".to_owned()]));
        } else {
            additions.push("\"theme\": {\"name\": \"slate-sync\"}".to_owned());
        }
        if self.property("$schema").is_none() {
            additions.push(format!("\"$schema\": \"{}\"", OpencodeAdapter::CLI_SCHEMA));
        }
        if !additions.is_empty() {
            edits.push(self.insertion(&self.root, &additions));
        }
        let output = self.edited(edits);
        if Document::parse(&output, path)?.has_slate_theme() != Some(true) {
            return Err(invalid(
                path,
                "cannot prepare OpenCode theme edit; contents omitted",
            ));
        }
        Ok(Some(output.into_bytes()))
    }

    /// Remove only Slate's theme name; preserve mode, comments and other settings.
    pub(crate) fn without_slate_theme(&self, path: &Path) -> Result<Option<Vec<u8>>> {
        if self.has_slate_theme() != Some(true) {
            return Ok(Some(self.text.as_bytes().to_vec()));
        }
        let only_name = self.theme().expect("theme object").properties.len() == 1;
        let removable = only_name
            && self.root.properties.iter().all(|p| {
                p.name.as_str() == "theme"
                    || (p.name.as_str() == "$schema"
                        && self.string("$schema") == Some(OpencodeAdapter::CLI_SCHEMA))
            });
        if removable && !self.tokens.iter().any(|t| comment(&t.token)) {
            return Ok(None);
        }
        let property = if only_name {
            self.property("theme")
        } else {
            self.theme_name()
        }
        .expect("Slate theme name");
        let mut edits: Vec<_> = self
            .tokens
            .iter()
            .filter(|t| {
                t.start() >= property.start() && t.end() <= property.end() && !comment(&t.token)
            })
            .map(|t| (t.start()..t.end(), String::new()))
            .collect();
        let following = self
            .tokens
            .iter()
            .find(|t| t.start() >= property.end() && !comment(&t.token));
        let comma = following
            .filter(|t| matches!(t.token, Token::Comma))
            .or_else(|| {
                self.tokens
                    .iter()
                    .rev()
                    .find(|t| t.end() <= property.start() && !comment(&t.token))
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
            Document::parse(text, path)?.slate(path)?
        } else {
            Document::parse("{\n}\n", path)?.slate(path)?
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

    pub(super) fn asset(path: &Path, bytes: Vec<u8>) -> Result<Self> {
        let destination = file_read::directory_alias_target(path)
            .ok_or_else(|| invalid(path, "cannot resolve OpenCode theme destination"))?;
        let original = source(path)?;
        if original
            .as_ref()
            .is_some_and(|source| !super::palette::is_owned(&source.bytes))
        {
            return Err(invalid(path, "slate-sync.json is not a Slate theme; choose another filename for the personal theme"));
        }
        let replacement = (!original
            .as_ref()
            .is_some_and(|source| source.bytes == bytes))
        .then_some(bytes);
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
