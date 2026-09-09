use crate::error::{Result, SlateError};

pub struct Setting {
    pub key: &'static str,
    pub description: &'static str,
    pub set_values: &'static [&'static str],
}

pub const SETTINGS: [Setting; 5] = [
    Setting {
        key: "opacity",
        description: "Saved opacity preset; unset does not imply a running terminal's opacity",
        set_values: &["solid", "frosted", "clear"],
    },
    Setting {
        key: "auto-theme",
        description: "Automatic theme preference; not watcher health or the dark/light pairing",
        set_values: &["enable", "disable", "configure"],
    },
    Setting {
        key: "fastfetch",
        description: "Fastfetch autorun preference; manual use remains available",
        set_values: &["enable", "disable"],
    },
    Setting {
        key: "sound",
        description: "Sound preference; quiet/auto/session behavior may suppress feedback",
        set_values: &["on", "off"],
    },
    Setting {
        key: "editor",
        description:
            "Permission for future Neovim setup activation; not the current editor or hooks",
        set_values: &["enable", "disable"],
    },
];

pub(super) fn escaped(text: &str) -> String {
    let mut output = String::new();
    for c in text.chars() {
        if c.is_control() || matches!(c, '\u{2028}'..='\u{202e}' | '\u{2066}'..='\u{2069}') {
            output.extend(c.escape_default());
        } else {
            output.push(c);
        }
    }
    output
}

fn argument(text: &str) -> String {
    let short: String = text.chars().take(120).collect();
    format!(
        "{}{}",
        escaped(&short),
        if short.len() < text.len() {
            "... (truncated)"
        } else {
            ""
        }
    )
}

pub(super) fn setting(key: &str) -> Result<&'static Setting> {
    SETTINGS
        .iter()
        .find(|setting| setting.key == key)
        .ok_or_else(|| {
            SlateError::InvalidConfig(format!(
                "Unknown config key: '{}'. Known keys: {}",
                argument(key),
                SETTINGS
                    .iter()
                    .map(|setting| setting.key)
                    .collect::<Vec<_>>()
                    .join(", ")
            ))
        })
}

pub fn validate_key(key: &str) -> Result<()> {
    setting(key).map(|_| ())
}

pub fn validate_set(key: &str, value: &str) -> Result<()> {
    let setting = setting(key)?;
    if setting.set_values.contains(&value) {
        return Ok(());
    }
    Err(SlateError::InvalidConfig(format!(
        "Invalid {} value/action: '{}'. Must be one of: {}",
        setting.key,
        argument(value),
        setting.set_values.join(", ")
    )))
}
