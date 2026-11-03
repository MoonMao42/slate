//! Explicit OpenCode colors avoid the system theme's unaudited gray and diff
//! ramps. The canvas stays transparent; raised surfaces have known colors.
use crate::{cli::picker::preview_panel::SemanticColor, error::Result, theme::ThemeVariant};
use serde_json::{json, Map, Value};

pub(crate) const SCHEMA: &str = "https://opencode.ai/theme.json";

pub(crate) fn render(theme: &ThemeVariant) -> Result<String> {
    theme.validate()?;
    let p = &theme.palette;
    let panel = p.resolve(SemanticColor::Surface);
    let element = p.resolve(SemanticColor::SurfaceAlt);
    let added = p.diff_background(&p.green);
    let removed = p.diff_background(&p.red);
    let backgrounds = [&p.background[..], &panel, &element, &added, &removed];
    let mut colors = Map::new();
    for (name, color) in [
        ("primary", &p.cyan),
        ("secondary", &p.magenta),
        ("accent", &p.cyan),
        ("error", &p.red),
        ("warning", &p.yellow),
        ("success", &p.green),
        ("info", &p.cyan),
        ("text", &p.foreground),
        ("textMuted", &p.bright_black),
        ("border", &p.bright_black),
        ("borderActive", &p.blue),
        ("borderSubtle", &p.bright_black),
        ("diffAdded", &p.green),
        ("diffRemoved", &p.red),
        ("diffContext", &p.foreground),
        ("diffHunkHeader", &p.blue),
        ("diffHighlightAdded", &p.bright_green),
        ("diffHighlightRemoved", &p.bright_red),
        ("diffLineNumber", &p.bright_black),
        ("markdownText", &p.foreground),
        ("markdownHeading", &p.blue),
        ("markdownLink", &p.blue),
        ("markdownLinkText", &p.cyan),
        ("markdownCode", &p.green),
        ("markdownBlockQuote", &p.bright_black),
        ("markdownEmph", &p.yellow),
        ("markdownStrong", &p.magenta),
        ("markdownHorizontalRule", &p.bright_black),
        ("markdownListItem", &p.blue),
        ("markdownListEnumeration", &p.cyan),
        ("markdownImage", &p.blue),
        ("markdownImageText", &p.cyan),
        ("markdownCodeBlock", &p.foreground),
        ("syntaxComment", &p.bright_black),
        ("syntaxKeyword", &p.magenta),
        ("syntaxFunction", &p.blue),
        ("syntaxVariable", &p.foreground),
        ("syntaxString", &p.green),
        ("syntaxNumber", &p.yellow),
        ("syntaxType", &p.cyan),
        ("syntaxOperator", &p.cyan),
        ("syntaxPunctuation", &p.foreground),
    ] {
        colors.insert(
            name.into(),
            crate::wcag::readable_on(color, &backgrounds).into(),
        );
    }
    // The list selection uses primary as its fill, after contrast adjustment.
    let selected = crate::wcag::readable_color(&p.background, colors["primary"].as_str().unwrap());
    for (key, color) in [
        ("selectedListItemText", selected.as_str()),
        ("background", "none"),
        ("backgroundPanel", &panel),
        ("backgroundElement", &element),
        ("backgroundMenu", &element),
        ("diffAddedBg", &added),
        ("diffRemovedBg", &removed),
        ("diffContextBg", "slateBackground"),
        ("diffAddedLineNumberBg", &added),
        ("diffRemovedLineNumberBg", &removed),
    ] {
        colors.insert(key.into(), color.into());
    }
    let output =
        json!({"$schema": SCHEMA, "defs": {"slateBackground": p.background}, "theme": colors});
    Ok(format!("{}\n", serde_json::to_string_pretty(&output)?))
}

pub(crate) fn is_owned(bytes: &[u8]) -> bool {
    serde_json::from_slice::<Value>(bytes).is_ok_and(|value| {
        value["$schema"] == SCHEMA
            && value["theme"]["background"] == "none"
            && value["theme"]["diffContextBg"] == "slateBackground"
            && value["defs"]["slateBackground"]
                .as_str()
                .is_some_and(|color| crate::wcag::hex_to_rgb(color).is_ok())
    })
}
