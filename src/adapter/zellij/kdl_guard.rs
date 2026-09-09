//! Protect KDL v1's recursive parser without rewriting users' configuration.
//! The private parse copy masks block-comment interiors at identical byte offsets;
//! caller edits still use exact spans against the original, untouched text.
//! This is a resource guard, not a replacement syntax parser. Never serialize its AST.
use super::config::invalid;
use crate::error::Result;
use kdl::KdlDocument;
use std::borrow::Cow;

// Slash-dash comments recurse even without child braces. Counting all of them is
// deliberately conservative, including ignored nodes/entries in separate branches.
const RECURSION_BUDGET: usize = 128;
const PARSER_STACK_BYTES: usize = 32 * 1024 * 1024;

pub(super) fn parse(text: &str) -> Result<KdlDocument> {
    let input = prepare(text)?;
    // A bounded dedicated stack also makes the limit independent of whether
    // callers are the main CLI, test threads, or smaller Rayon adapter workers.
    std::thread::scope(|scope| {
        let worker = std::thread::Builder::new()
            .name("slate-kdl-parser".into())
            .stack_size(PARSER_STACK_BYTES)
            .spawn_scoped(scope, || {
                input
                    .parse()
                    .map_err(|_| invalid("invalid KDL v1 configuration"))
            })
            .map_err(|_| invalid("cannot start the bounded KDL parser worker"))?;
        worker
            .join()
            .map_err(|_| invalid("KDL parser did not complete normally"))?
    })
}

fn prepare(text: &str) -> Result<Cow<'_, str>> {
    let bytes = text.as_bytes();
    let mut masked: Option<Vec<u8>> = None;
    let (mut i, mut depth, mut max_depth, mut slashdashes) = (0, 0usize, 0, 0);
    while i < bytes.len() {
        if bytes[i..].starts_with(b"//") {
            i += 2;
            while i < bytes.len() && !newline(&bytes[i..]) {
                i += 1;
            }
        } else if bytes[i..].starts_with(b"/*") {
            let start = i;
            let mut comments = 1usize;
            i += 2;
            while i < bytes.len() && comments > 0 {
                if bytes[i..].starts_with(b"/*") {
                    comments += 1;
                    i += 2;
                } else if bytes[i..].starts_with(b"*/") {
                    comments -= 1;
                    i += 2;
                } else {
                    i += 1;
                }
            }
            if comments != 0 {
                return Err(invalid("unterminated KDL block comment"));
            }
            // kdl 4.x recursively processes not only nested comments, but every
            // slash/star segment inside a flat comment. One space run removes
            // both recursion hazards. Byte length and all value spans are kept.
            masked.get_or_insert_with(|| bytes.to_vec())[start + 2..i - 2].fill(b' ');
        } else if bytes[i] == b'r' && raw_open(bytes, i).is_some() {
            let (start, hashes) = raw_open(bytes, i).expect("checked raw opener");
            i = start;
            loop {
                if i >= bytes.len() {
                    return Err(invalid("unterminated KDL raw string"));
                }
                if bytes[i] == b'"' {
                    let mut end = i + 1;
                    while end < bytes.len() && bytes[end] == b'#' {
                        end += 1;
                    }
                    if end - i > hashes {
                        i += 1 + hashes;
                        break;
                    }
                    i = end;
                } else {
                    i += 1;
                }
            }
        } else if bytes[i] == b'"' {
            i += 1;
            loop {
                if i >= bytes.len() {
                    return Err(invalid("unterminated KDL string"));
                }
                match bytes[i] {
                    b'\\' => i += 2,
                    b'"' => {
                        i += 1;
                        break;
                    }
                    _ => i += 1,
                }
            }
        } else if bytes[i..].starts_with(b"/-") {
            slashdashes += 1;
            i += 2;
        } else {
            match bytes[i] {
                b'{' => {
                    depth += 1;
                    max_depth = max_depth.max(depth);
                }
                b'}' => depth = depth.saturating_sub(1),
                _ => {}
            }
            i += 1;
        }
        if max_depth + slashdashes > RECURSION_BUDGET {
            return Err(invalid(
                "KDL complexity exceeds 128 (maximum child nesting plus slash-dash comment count)",
            ));
        }
    }
    Ok(match masked {
        Some(bytes) => {
            Cow::Owned(String::from_utf8(bytes).expect("whole comment interiors were masked"))
        }
        None => Cow::Borrowed(text),
    })
}

fn raw_open(bytes: &[u8], at: usize) -> Option<(usize, usize)> {
    let mut end = at + 1;
    while end < bytes.len() && bytes[end] == b'#' {
        end += 1;
    }
    (bytes.get(end) == Some(&b'"')).then_some((end + 1, end - at - 1))
}

fn newline(bytes: &[u8]) -> bool {
    matches!(bytes[0], b'\r' | b'\n' | 0x0c)
        || bytes.starts_with(b"\xc2\x85")
        || bytes.starts_with(b"\xe2\x80\xa8")
        || bytes.starts_with(b"\xe2\x80\xa9")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guard_ignores_structure_in_strings_and_all_kdl_line_comments() {
        for end in [
            "\r\n", "\n", "\r", "\u{0085}", "\u{000c}", "\u{2028}", "\u{2029}",
        ] {
            let text = format!("// {}{end}theme \"nord\"\n", "{ /- ".repeat(1000));
            assert!(prepare(&text).is_ok());
            assert!(parse(&text).unwrap().get("theme").is_some());
        }
        for value in [
            format!("\"{}\"", "{{ /- /* \\\" ".repeat(1000)),
            format!("r###\"{}\"###", "{{ /- /* \\\"## ".repeat(1000)),
        ] {
            let text = format!("node {value}\n");
            assert_eq!(prepare(&text).unwrap(), text);
            assert!(parse(&text).is_ok());
        }
    }

    #[test]
    fn guard_preserves_byte_offsets_and_does_not_accept_invalid_syntax() {
        let text = "/* 中文 /* nested */\r\n** / **/theme /*中*/ r#\"nord\"#\n";
        let prepared = prepare(text).unwrap();
        assert_eq!(prepared.len(), text.len());
        let doc = parse(text).unwrap();
        let entry = &doc.get("theme").unwrap().entries()[0];
        let span = entry.span();
        assert_eq!(
            &text[span.offset()..span.offset() + span.len()],
            "r#\"nord\"#"
        );
        for bad in [
            "node {",
            "node /* unfinished",
            "node r#\"unfinished",
            "node \"bad\\z\"",
            "node \"unfinished",
        ] {
            assert!(parse(bad).is_err(), "{bad}");
        }
    }
}
