use super::{parse_import_intent, ImportRequest, ToolImportFlags};
use crate::error::Result;
use serde::Serialize;
use std::io::Write;

const NOTES: [&str; 4] = [
    "This previews requested settings, not a diff against your current configuration.",
    "Theme/font/opacity: none keeps the current value (null in JSON). Omitted tool flags mean disable.",
    "Font availability and write permissions are not checked. Applying may download a catalog font.",
    "Import first saves a file recovery point and applies settings in steps; a later failure does not automatically roll back earlier changes. File recovery excludes font installations, external caches and running application state.",
];

#[derive(Serialize)]
struct ImportPreview<'a> {
    schema_version: u8,
    scope: &'static str,
    theme: Option<&'a str>,
    font: Option<&'a str>,
    opacity: Option<String>,
    tools: ToolImportFlags,
    notes: &'static [&'static str],
}

impl<'a> From<&'a ImportRequest> for ImportPreview<'a> {
    fn from(request: &'a ImportRequest) -> Self {
        Self {
            schema_version: 1,
            scope: "requested_settings",
            theme: request.theme.as_deref(),
            font: request.font.as_deref(),
            opacity: request
                .opacity
                .map(|value| value.to_string().to_lowercase()),
            tools: request.tools,
            notes: &NOTES,
        }
    }
}

/// Available without HOME or a readable profile. No font/config/lock/sound probes.
pub fn handle_import_preview(uri: &str, json: bool) -> Result<()> {
    let request = parse_import_intent(uri)?;
    let plan = ImportPreview::from(&request);
    let output = if json {
        format!("{}\n", serde_json::to_string_pretty(&plan)?)
    } else {
        let font = plan
            .font
            .map(|value| format!("{value:?}"))
            .unwrap_or_else(|| "keep current".into());
        let flag = |enabled| if enabled { "enable" } else { "disable" };
        let mut text = format!(
            "Import preview — no changes made\n\nTheme: {}\nFont request: {}\nOpacity: {}\nStarship: {}\nZsh highlighting: {}\nFastfetch startup: {}\n\n",
            plan.theme.unwrap_or("keep current"),
            font,
            plan.opacity.as_deref().unwrap_or("keep current"),
            flag(plan.tools.starship),
            flag(plan.tools.highlighting),
            flag(plan.tools.fastfetch),
        );
        for note in NOTES {
            text.push_str(note);
            text.push('\n');
        }
        text
    };
    match std::io::stdout().lock().write_all(output.as_bytes()) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::BrokenPipe => Ok(()),
        Err(error) => Err(error.into()),
    }
}
