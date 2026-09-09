use super::{
    codec, parse_opacity_segment, parse_theme_segment, validate_font_name, ImportRequest,
    ToolImportFlags,
};
use crate::brand::{render_context::RenderContext, roles::Roles};
use crate::config::file_read::{self, Links, MAX_DOCUMENT_BYTES, MAX_STATE_BYTES};
use crate::env::SlateEnv;
use crate::error::{Result, SlateError};
use crate::theme::{ThemeRegistry, DEFAULT_THEME_ID};
use std::fs;
use std::io::Write;
use std::path::Path;

fn invalid(field: &str) -> SlateError {
    SlateError::InvalidConfig(format!("Cannot export {field}: inspect its saved value, file type, size and permissions. No settings were changed."))
}

// Read each relevant file once using the shared bounded reader, but retain
// export's stricter final-symlink policy and content-free field diagnostics.
// This is not a transactional snapshot across concurrent configuration writes.
fn read_optional(path: &Path, limit: u64, field: &str) -> Result<Option<String>> {
    file_read::read(path, limit, Links::Reject)
        .map_err(|_| invalid(field))?
        .map(|source| String::from_utf8(source.bytes).map_err(|_| invalid(field)))
        .transpose()
}

fn tracking(env: &SlateEnv, file: &str, field: &str) -> Result<Option<String>> {
    Ok(
        read_optional(&env.managed_file(file), MAX_STATE_BYTES, field)?
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty()),
    )
}

fn tool_flag(doc: &toml::Value, key: &str) -> Result<bool> {
    let Some(tools) = doc.get("tools") else {
        return Ok(true);
    };
    let tools = tools
        .as_table()
        .ok_or_else(|| invalid("config.toml [tools]"))?;
    tools.get(key).map_or(Ok(true), |value| {
        value
            .as_bool()
            .ok_or_else(|| invalid("config.toml tool flags"))
    })
}

fn saved_request(env: &SlateEnv) -> Result<ImportRequest> {
    let theme = tracking(env, "current", "saved theme")?;
    if let Some(theme) = &theme {
        // A tracking file containing the URI sentinel is not a known theme.
        if parse_theme_segment(theme)
            .map_err(|_| invalid("saved theme"))?
            .is_none()
        {
            return Err(invalid("saved theme"));
        }
    }
    let font = tracking(env, "current-font", "saved font")?;
    if let Some(font) = &font {
        validate_font_name(font).map_err(|_| invalid("saved font"))?;
    }
    let opacity = tracking(env, "current-opacity", "saved opacity")?
        .map(|value| {
            parse_opacity_segment(&value)
                .map_err(|_| invalid("saved opacity"))?
                .ok_or_else(|| invalid("saved opacity"))
        })
        .transpose()?;
    let preferences = read_optional(
        &env.managed_file("config.toml"),
        MAX_DOCUMENT_BYTES,
        "config.toml",
    )?
    .unwrap_or_default();
    let doc: toml::Value = preferences.parse().map_err(|_| invalid("config.toml"))?;
    let fastfetch = match fs::symlink_metadata(env.managed_file("autorun-fastfetch")) {
        Ok(metadata) if metadata.is_file() => true,
        Ok(_) => return Err(invalid("autorun-fastfetch marker")),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
        Err(_) => return Err(invalid("autorun-fastfetch marker")),
    };
    Ok(ImportRequest {
        theme,
        font,
        opacity,
        tools: ToolImportFlags {
            starship: tool_flag(&doc, "starship")?,
            highlighting: tool_flag(&doc, "zsh_highlighting")?,
            fastfetch,
        },
    })
}

fn encode(request: &ImportRequest) -> String {
    let font = request
        .font
        .as_deref()
        .map(codec::encode_font)
        .unwrap_or_else(|| "none".into());
    let opacity = request
        .opacity
        .map(|value| value.to_string().to_lowercase())
        .unwrap_or_else(|| "none".into());
    let tools = [
        ("s", request.tools.starship),
        ("h", request.tools.highlighting),
        ("f", request.tools.fastfetch),
    ]
    .into_iter()
    .filter_map(|(name, enabled)| enabled.then_some(name))
    .collect::<Vec<_>>()
    .join(",");
    format!(
        "slate://v1/{}/{font}/{opacity}/{}",
        request.theme.as_deref().unwrap_or("none"),
        if tools.is_empty() { "none" } else { &tools }
    )
}

pub(crate) fn build_export_uri(env: &SlateEnv) -> Result<String> {
    saved_request(env).map(|request| encode(&request))
}

pub fn handle_export_with_options(raw: bool) -> Result<()> {
    let env = SlateEnv::from_process()?;
    let request = saved_request(&env)?;
    let uri = encode(&request);
    let output = if raw {
        format!("{uri}\n")
    } else {
        // Style from the already-validated theme, not another profile read.
        let registry = ThemeRegistry::new()?;
        let theme = registry
            .get(request.theme.as_deref().unwrap_or(DEFAULT_THEME_ID))
            .ok_or_else(|| invalid("saved theme"))?;
        let context = RenderContext::new(theme);
        let roles = Roles::new(&context);
        let styled = roles.path(&uri);
        format!("\n  {styled}\n\n  Preview before applying:\n  slate import '{styled}' --dry-run\n  Remove --dry-run to apply the settings.\n\n")
    };
    match std::io::stdout().lock().write_all(output.as_bytes()) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::BrokenPipe => Ok(()),
        Err(error) => Err(error.into()),
    }
}
