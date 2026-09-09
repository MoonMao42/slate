//! A family name is data, not terminal configuration or a font-spec expression.
//! Each terminal has a different grammar; never apply one generic quote escape.
use crate::error::{Result, SlateError};

pub(crate) fn validate_family(family: &str) -> Result<()> {
    if family.len() > 256
        || family
            .chars()
            .any(|c| c.is_control() || matches!(c, '\u{2028}' | '\u{2029}'))
        || !family.chars().any(char::is_alphanumeric)
    {
        return Err(SlateError::InvalidConfig(
            "Font must be a nonempty family name of at most 256 bytes, without control characters or line separators.".into(),
        ));
    }
    Ok(())
}

pub(crate) fn alacritty(family: &str) -> Result<String> {
    validate_family(family)?;
    // Use TOML's serializer for both dedicated font files and theme reapplication.
    Ok(format!(
        "[font.normal]\nfamily = {}\n",
        toml_edit::Value::from(family)
    ))
}

pub(crate) fn ghostty(family: &str) -> Result<String> {
    validate_family(family)?;
    // Ghostty's LineIterator removes exactly one pair of surrounding quotes;
    // it does NOT decode backslash escapes in ordinary config values.
    // https://github.com/ghostty-org/ghostty/blob/v1.3.1/src/cli/args.zig
    Ok(format!("font-family = \"{family}\"\n"))
}

pub(crate) fn kitty(family: &str) -> Result<String> {
    validate_family(family)?;
    // Keep ordinary names compatible with older Kitty. For ambiguous names,
    // use the explicit, shell-quoted family syntax introduced in Kitty 0.36.
    // In particular, `auto` must stay a family, and `=` must not become an axis.
    // https://github.com/kovidgoyal/kitty/blob/v0.36.0/kitty/fonts/__init__.py
    let simple = family != "auto"
        && family
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, ' ' | '-' | '_' | '.'));
    let value = if simple {
        family.to_owned()
    } else {
        format!("family={}", crate::detection::shell_quote(family))
    };
    Ok(format!("font_family {value}\n"))
}
