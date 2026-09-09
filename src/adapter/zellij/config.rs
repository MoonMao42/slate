//! Parse KDL v1, replace only exact value spans, and publish assets first.
use super::{palette, ZellijAdapter};
use crate::{
    config::{
        atomic_write_synced,
        file_read::{self, Links, Source, MAX_TOOL_CONFIG_BYTES},
        recovery_paths,
    },
    env::SlateEnv,
    error::{Result, SlateError},
    theme::ThemeVariant,
};
use kdl::{KdlDocument, KdlNode};
use std::{
    fs,
    path::{Path, PathBuf},
};

const NAME: &str = "slate-sync";
const CHOICES: [&str; 3] = ["theme", "theme_dark", "theme_light"];

pub(super) fn invalid(reason: &str) -> SlateError {
    SlateError::InvalidConfig(format!("Zellij: {reason}; file contents omitted"))
}
fn parse(bytes: &[u8]) -> Result<KdlDocument> {
    if bytes.len() as u64 > MAX_TOOL_CONFIG_BYTES {
        return Err(invalid("configuration exceeds 8 MiB"));
    }
    let text = std::str::from_utf8(bytes).map_err(|_| invalid("configuration is not UTF-8"))?;
    super::kdl_guard::parse(text)
}

fn binding<'a>(doc: &'a KdlDocument, name: &str) -> Result<Option<&'a KdlNode>> {
    let mut nodes = doc
        .nodes()
        .iter()
        .filter(|node| node.name().value() == name);
    let Some(node) = nodes.next() else {
        return Ok(None);
    };
    if nodes.next().is_some() {
        return Err(invalid("duplicate theme or theme_dir choices"));
    }
    if node.ty().is_some()
        || node.children().is_some()
        || node.entries().len() != 1
        || node.entries()[0].name().is_some()
        || node.entries()[0].ty().is_some()
        || node.entries()[0].value().as_string().is_none()
    {
        return Err(invalid(
            "theme and theme_dir choices require one untyped string",
        ));
    }
    Ok(Some(node))
}

fn edit(bytes: &[u8], clean: bool) -> Result<Vec<u8>> {
    let doc = parse(bytes)?;
    let mut replacements = Vec::new();
    let mut missing = Vec::new();
    for key in CHOICES {
        if let Some(node) = binding(&doc, key)? {
            let entry = &node.entries()[0];
            let value = entry.value().as_string().expect("checked string");
            if (!clean && value != NAME) || (clean && value == NAME) {
                let span = entry.span();
                replacements.push((
                    span.offset()..span.offset() + span.len(),
                    if clean {
                        "\"default\""
                    } else {
                        "\"slate-sync\""
                    },
                ));
            }
        } else if !clean {
            missing.push(key);
        }
    }
    let mut output = std::str::from_utf8(bytes)
        .expect("validated UTF-8")
        .to_owned();
    replacements.sort_by_key(|(range, _)| std::cmp::Reverse(range.start));
    for (range, value) in replacements {
        output.replace_range(range, value);
    }
    let newline = if output.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    for key in missing {
        if !output.is_empty() && !output.ends_with('\n') {
            output.push_str(newline);
        }
        output.push_str(&format!("{key} \"{NAME}\"{newline}"));
    }
    parse(output.as_bytes())?;
    Ok(output.into_bytes())
}

pub(crate) fn clean_config(bytes: &[u8]) -> Result<Vec<u8>> {
    edit(bytes, true)
}
pub(crate) fn owns_theme(bytes: &[u8]) -> bool {
    bytes.starts_with(palette::HEADER.as_bytes())
}

pub(crate) fn generated_theme(theme: &ThemeVariant) -> Result<String> {
    palette::render(theme)
}

pub(crate) fn inspect_selection(bytes: &[u8]) -> Result<[bool; 3]> {
    let doc = parse(bytes)?;
    let mut selected = [false; 3];
    for (index, key) in CHOICES.into_iter().enumerate() {
        selected[index] = binding(&doc, key)?
            .is_some_and(|node| node.entries()[0].value().as_string() == Some(NAME));
    }
    Ok(selected)
}

#[derive(Clone)]
struct File {
    path: PathBuf,
    resolved: PathBuf,
    source: Option<Source>,
}
impl File {
    fn read(env: &SlateEnv, path: PathBuf) -> Result<Self> {
        recovery_paths::validate_file_path(env, &path, "Zellij")?;
        let resolved = file_read::directory_alias_target(&path)
            .ok_or_else(|| invalid("cannot resolve destination"))?;
        let source = file_read::read(&path, MAX_TOOL_CONFIG_BYTES, Links::Reject)
            .map_err(|_| invalid("unsafe or unreadable configuration file"))?;
        Ok(Self {
            path,
            resolved,
            source,
        })
    }
    fn bytes(&self) -> &[u8] {
        self.source.as_ref().map_or(&[], |s| s.bytes.as_slice())
    }
    fn verify(&self, env: &SlateEnv) -> Result<()> {
        let current = Self::read(env, self.path.clone())?;
        if current.resolved != self.resolved || current.source != self.source {
            return Err(invalid(
                "configuration changed while preparing; review again",
            ));
        }
        Ok(())
    }
    fn publish(&self, env: &SlateEnv, bytes: &[u8]) -> Result<()> {
        self.verify(env)?;
        if self.source.as_ref().is_some_and(|s| s.bytes == bytes) {
            return Ok(());
        }
        fs::create_dir_all(self.path.parent().expect("absolute parent"))?;
        self.verify(env)?;
        atomic_write_synced(&self.path, bytes)
    }
}

pub(crate) fn theme_path(env: &SlateEnv, bytes: &[u8]) -> Result<PathBuf> {
    let doc = parse(bytes)?;
    let dir = match binding(&doc, "theme_dir")? {
        Some(node) => PathBuf::from(
            node.entries()[0]
                .value()
                .as_string()
                .expect("checked string"),
        ),
        None => env.zellij_paths()?.0.join("themes"),
    };
    if !dir.is_absolute() {
        return Err(invalid(
            "relative or tilde theme_dir is not a safe write target; use an absolute path",
        ));
    }
    Ok(dir.join("slate-sync.kdl"))
}

pub(super) fn paths(env: &SlateEnv) -> Result<[PathBuf; 2]> {
    let config = File::read(env, ZellijAdapter::config_path(env)?)?;
    let asset = theme_path(env, config.bytes())?;
    recovery_paths::validate_file_path(env, &asset, "Zellij")?;
    let resolved = file_read::directory_alias_target(&asset)
        .ok_or_else(|| invalid("cannot resolve theme destination"))?;
    env.verify_zellij_destinations([(&config.path, &config.resolved), (&asset, &resolved)])?;
    Ok([config.path, asset])
}

fn contains_theme(doc: &KdlDocument) -> bool {
    doc.nodes()
        .iter()
        .filter(|node| node.name().value() == "themes")
        .filter_map(KdlNode::children)
        .any(|themes| {
            themes
                .nodes()
                .iter()
                .any(|theme| theme.name().value() == NAME)
        })
}

pub(crate) fn check_collisions(env: &SlateEnv, bytes: &[u8], asset_path: &Path) -> Result<()> {
    // Callers doing read-only diagnostics have not necessarily validated paths.
    // Do not even enumerate a theme directory outside an isolated profile.
    recovery_paths::validate_file_path(env, asset_path, "Zellij")?;
    if contains_theme(&parse(bytes)?) {
        return Err(invalid(
            "an inline slate-sync theme already exists; preserve or rename it before connecting",
        ));
    }
    let entries = match fs::read_dir(asset_path.parent().expect("parent")) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(_) => return Err(invalid("cannot inspect theme directory")),
    };
    let mut total = 0;
    for (count, entry) in entries.enumerate() {
        if count >= 256 {
            return Err(invalid(
                "theme directory exceeds the 256-entry inspection limit",
            ));
        }
        let entry = entry.map_err(|_| invalid("cannot inspect theme directory entry"))?;
        let path = entry.path();
        if path == asset_path || path.extension().is_none_or(|ext| ext != "kdl") {
            continue;
        }
        let file = File::read(env, path)?;
        total += file.bytes().len();
        if total as u64 > MAX_TOOL_CONFIG_BYTES {
            return Err(invalid(
                "other theme files exceed the 8 MiB inspection limit",
            ));
        }
        if contains_theme(&parse(file.bytes())?) {
            return Err(invalid(
                "another theme file defines slate-sync; preserve or rename it before connecting",
            ));
        }
    }
    Ok(())
}

pub(super) fn apply(env: &SlateEnv, theme: &ThemeVariant) -> Result<()> {
    let rendered = palette::render(theme)?;
    let [config_path, asset_path] = paths(env)?;
    recovery_paths::targets(env, [config_path.clone(), asset_path.clone()], "Zellij")?;
    let config = File::read(env, config_path)?;
    if theme_path(env, config.bytes())? != asset_path {
        return Err(invalid("theme directory changed; review again"));
    }
    let asset = File::read(env, asset_path.clone())?;
    if asset.source.is_some() && !owns_theme(asset.bytes()) {
        return Err(invalid("slate-sync.kdl exists without Slate ownership; preserve or rename it before connecting"));
    }
    let selected = edit(config.bytes(), false)?;
    env.verify_zellij_destinations([
        (&config.path, &config.resolved),
        (&asset.path, &asset.resolved),
    ])?;
    check_collisions(env, config.bytes(), &asset_path)?;
    config.verify(env)?;
    asset.publish(env, rendered.as_bytes())?;
    env.zellij_paths()?;
    check_collisions(env, config.bytes(), &asset_path)?;
    config.publish(env, &selected)
}

#[cfg(test)]
mod tests;
