//! Literal Kitty directive inspection, shared by installation and cleanup.
//! Continuations belong to their preceding physical line. No includes are read,
//! variables expanded, generators run, or retained bytes reserialized.
use std::borrow::Cow;

pub(crate) struct Line<'a> {
    pub(crate) raw: &'a [u8],
    text: Cow<'a, [u8]>,
}

fn trim_start(bytes: &[u8]) -> &[u8] {
    // Decode only the valid prefix: binary user bytes elsewhere are retained,
    // and a Unicode-indented continuation still belongs to its preceding line.
    let prefix = match std::str::from_utf8(bytes) {
        Ok(text) => text,
        Err(error) => std::str::from_utf8(&bytes[..error.valid_up_to()]).unwrap_or(""),
    };
    &bytes[prefix.len() - prefix.trim_start().len()..]
}

fn trim(bytes: &[u8]) -> &[u8] {
    let bytes = trim_start(bytes);
    if let Ok(text) = std::str::from_utf8(bytes) {
        return &bytes[..text.trim_end().len()];
    }
    &bytes[..bytes
        .iter()
        .rposition(|b| !b.is_ascii_whitespace())
        .map_or(0, |i| i + 1)]
}

fn without_newline(bytes: &[u8]) -> &[u8] {
    bytes
        .strip_suffix(b"\r\n")
        .or_else(|| bytes.strip_suffix(b"\n"))
        .unwrap_or(bytes)
}

impl Line<'_> {
    pub(crate) fn value(&self, key: &[u8]) -> Option<&[u8]> {
        let remainder = trim(&self.text).strip_prefix(key)?;
        if trim_start(remainder).len() == remainder.len() {
            return None;
        }
        let value = trim(remainder);
        (!value.is_empty()).then_some(value)
    }
}

pub(crate) fn lines(mut bytes: &[u8]) -> impl Iterator<Item = Line<'_>> {
    std::iter::from_fn(move || {
        if bytes.is_empty() {
            return None;
        }
        let mut physical = bytes.split_inclusive(|b| *b == b'\n');
        let first = physical.next()?;
        let mut used = first.len();
        let mut text = Cow::Borrowed(without_newline(first));
        for next in physical {
            let Some(continuation) = trim_start(without_newline(next)).strip_prefix(b"\\") else {
                break;
            };
            text.to_mut().extend_from_slice(continuation);
            used += next.len();
        }
        let raw = &bytes[..used];
        bytes = &bytes[used..];
        Some(Line { raw, text })
    })
}
