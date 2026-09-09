//! Versioned font segments use UTF-8 byte percent-encoding (RFC 3986 §2.1/2.3).
//! Legacy four-part codes are never decoded, preserving literal percent signs.
use crate::error::{Result, SlateError};
use std::fmt::Write;

fn unreserved(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~')
}

pub(super) fn encode_font(font: &str) -> String {
    // Distinguish an actual font family named "none" from the keep-current token.
    if font == "none" {
        return "%6Eone".into();
    }
    let mut encoded = String::new();
    for byte in font.bytes() {
        if unreserved(byte) {
            encoded.push(char::from(byte));
        } else {
            write!(encoded, "%{byte:02X}").expect("writing to String cannot fail");
        }
    }
    encoded
}

pub(super) fn decode_font(segment: &str) -> Result<String> {
    let bad = || {
        SlateError::InvalidConfig(
            "Malformed v1 font encoding; use UTF-8 percent-encoding for reserved characters."
                .into(),
        )
    };
    let mut decoded = Vec::new();
    let mut bytes = segment.bytes();
    while let Some(byte) = bytes.next() {
        if byte == b'%' {
            let high = char::from(bytes.next().ok_or_else(bad)?)
                .to_digit(16)
                .ok_or_else(bad)?;
            let low = char::from(bytes.next().ok_or_else(bad)?)
                .to_digit(16)
                .ok_or_else(bad)?;
            decoded.push((high * 16 + low) as u8);
        } else if unreserved(byte) {
            decoded.push(byte);
        } else {
            return Err(bad());
        }
    }
    String::from_utf8(decoded).map_err(|_| bad())
}
