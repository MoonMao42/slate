//! OpenCode 2 semantic tokens, including interaction and dialog surfaces.
use crate::{
    cli::picker::preview_panel::SemanticColor,
    error::Result,
    theme::{ThemeAppearance, ThemeVariant},
    wcag::{blend_hex, readable_color, readable_on},
};
use serde_json::{json, Map, Value};

pub(crate) const SCHEMA: &str = "https://opencode.ai/theme.json";

pub(crate) fn render(theme: &ThemeVariant) -> Result<String> {
    theme.validate()?;
    let p = &theme.palette;
    let panel = p.resolve(SemanticColor::Surface);
    let element = p.resolve(SemanticColor::SurfaceAlt);
    let added = p.diff_background(&p.green);
    let removed = p.diff_background(&p.red);
    let warning = p.diff_background(&p.yellow);
    let info = p.diff_background(&p.cyan);
    let backgrounds = [
        p.background.as_str(),
        &panel,
        &element,
        &added,
        &removed,
        &warning,
        &info,
    ];
    let text = |color: &str| readable_on(color, &backgrounds);
    let primary = text(&p.cyan);
    let selected = readable_color(&p.background, &primary);
    let error = text(&p.red);
    let destructive = readable_color(&p.background, &error);
    let mut hue = Map::new();
    for (name, color) in [
        ("gray", &p.foreground),
        ("red", &p.red),
        ("orange", p.extras.get("orange").unwrap_or(&p.yellow)),
        ("yellow", &p.yellow),
        ("green", &p.green),
        ("cyan", &p.cyan),
        ("blue", &p.blue),
        ("purple", &p.magenta),
    ] {
        let mut scale = Map::new();
        for step in 1..=9 {
            // Early steps are used as text; later steps are tinted surfaces.
            let value = if step <= 4 {
                text(&blend_hex(color, &p.foreground, f64::from(4 - step) * 0.08))
            } else {
                blend_hex(&p.background, color, f64::from(9 - step) * 0.025)
            };
            scale.insert((step * 100).to_string(), value.into());
        }
        hue.insert(name.into(), scale.into());
    }
    for (name, reference) in [
        ("accent", "$hue.cyan"),
        ("interactive", "$hue.blue"),
        ("neutral", "$hue.gray"),
    ] {
        hue.insert(name.into(), reference.into());
    }
    let mut base = json!({
        "categorical": ["blue", "purple", "green", "orange", "red"],
        "text": {
            "base": text(&p.foreground), "muted": text(&p.bright_black), "faint": text(&p.bright_black),
            "action": {
                "primary": {"base": text(&p.foreground), "$hovered": text(&p.foreground), "$focused": selected, "$selected": selected, "$disabled": text(&p.bright_black)},
                "secondary": {"base": text(&p.bright_black), "$hovered": text(&p.foreground)},
                "destructive": {"base": destructive}
            },
            "formfield": {"base": text(&p.foreground), "$disabled": text(&p.bright_black)},
            "feedback": {
                "error": {"base": error}, "warning": {"base": text(&p.yellow)},
                "success": {"base": text(&p.green)}, "info": {"base": primary}
            }
        },
        "background": {
            "base": "transparent", "raised": {"base": panel, "high": element, "max": element},
            "action": {
                "primary": {"base": "transparent", "$hovered": element, "$focused": primary, "$selected": primary},
                "secondary": {"base": "transparent", "$hovered": element},
                "destructive": {"base": error}
            },
            "formfield": {"base": element},
            "feedback": {"error": {"base": removed}, "warning": {"base": warning}, "success": {"base": added}, "info": {"base": info}}
        },
        "border": {"base": text(&p.bright_black)}, "scrollbar": {"base": text(&p.blue)},
        "diff": {
            "text": {"added": text(&p.green), "removed": text(&p.red), "context": text(&p.foreground), "hunkHeader": text(&p.blue)},
            "background": {"added": added, "removed": removed, "context": p.background},
            "highlight": {"added": text(&p.bright_green), "removed": text(&p.bright_red)},
            "lineNumber": {"text": text(&p.bright_black), "background": {"added": added, "removed": removed}}
        },
        "@dialog": {"background": {"base": panel}}
    });
    for (group, entries) in [
        (
            "syntax",
            vec![
                ("comment", &p.bright_black),
                ("keyword", &p.magenta),
                ("function", &p.blue),
                ("variable", &p.foreground),
                ("string", &p.green),
                ("number", &p.yellow),
                ("type", &p.cyan),
                ("operator", &p.cyan),
                ("punctuation", &p.foreground),
            ],
        ),
        (
            "markdown",
            vec![
                ("text", &p.foreground),
                ("heading", &p.blue),
                ("link", &p.blue),
                ("linkText", &p.cyan),
                ("code", &p.green),
                ("blockQuote", &p.bright_black),
                ("emphasis", &p.yellow),
                ("strong", &p.magenta),
                ("horizontalRule", &p.bright_black),
                ("listItem", &p.blue),
                ("listEnumeration", &p.cyan),
                ("image", &p.blue),
                ("imageText", &p.cyan),
                ("codeBlock", &p.foreground),
            ],
        ),
    ] {
        base[group] = entries
            .into_iter()
            .map(|(key, color)| (key.to_owned(), Value::from(text(color))))
            .collect::<Map<_, _>>()
            .into();
    }
    let mode = match theme.appearance {
        ThemeAppearance::Dark => "dark",
        ThemeAppearance::Light => "light",
    };
    let output = json!({"$schema": SCHEMA, "defs": {"slateBackground": p.background}, "base": base, mode: {"hue": hue}});
    Ok(format!("{}\n", serde_json::to_string_pretty(&output)?))
}

pub(crate) fn is_owned(bytes: &[u8]) -> bool {
    serde_json::from_slice::<Value>(bytes).is_ok_and(|value| {
        value["$schema"] == SCHEMA
            && value["base"]["background"]["base"] == "transparent"
            && value["base"]["diff"]["background"]["context"] == value["defs"]["slateBackground"]
            && value["defs"]["slateBackground"]
                .as_str()
                .is_some_and(|color| crate::wcag::hex_to_rgb(color).is_ok())
    })
}
