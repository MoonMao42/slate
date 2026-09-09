//! Prepare an Alacritty integration edit before publishing managed files.
//! Keep import location/precedence; theme application is not a config migration.
use crate::config::atomic_write_synced;
use crate::config::file_read::{self, Links, Source, MAX_TOOL_CONFIG_BYTES};
use crate::error::{Result, SlateError};
use std::path::{Path, PathBuf};
use toml_edit::{Array, DocumentMut, Item, Table, Value};

fn invalid(path: &Path, reason: &str) -> SlateError {
    SlateError::ConfigReadError(path.display().to_string(), reason.to_owned())
}

fn source(path: &Path) -> Result<Option<Source>> {
    file_read::read(path, MAX_TOOL_CONFIG_BYTES, Links::Reject)
        .map_err(|error| invalid(path, &error.to_string()))
}

/// Alacritty selects the root import when present, even when it is empty.
/// An inactive general.import must not count as an installed Slate hook.
pub(crate) fn effective_import(doc: &DocumentMut) -> Option<&Item> {
    doc.get("import")
        .or_else(|| doc.get("general").and_then(|general| general.get("import")))
}

pub(crate) fn inspect_managed_source(
    path: &Path,
    original: Source,
    managed_path: &Path,
) -> Result<bool> {
    let document = Document::from_source(path, original)?;
    let Some(managed) = managed_path.to_str() else {
        return Err(invalid(path, "managed import path is not UTF-8"));
    };
    Ok(effective_import(&document.doc)
        .and_then(Item::as_array)
        .is_some_and(|array| array.iter().any(|value| value.as_str() == Some(managed))))
}

pub(super) struct Document {
    path: PathBuf,
    original: Source,
    doc: DocumentMut,
}

impl Document {
    pub(super) fn read(path: &Path) -> Result<Option<Self>> {
        let Some(original) = source(path)? else {
            return Ok(None);
        };
        Self::from_source(path, original).map(Some)
    }

    fn from_source(path: &Path, original: Source) -> Result<Self> {
        let text = std::str::from_utf8(&original.bytes).map_err(|error| {
            invalid(
                path,
                &format!(
                    "expected UTF-8 at byte {}; file contents omitted",
                    error.valid_up_to()
                ),
            )
        })?;
        let doc: DocumentMut = text
            .parse()
            .map_err(|_| invalid(path, "invalid Alacritty TOML; file contents omitted"))?;
        // Validate only the selected import list. Never activate, merge, or
        // replace a shadowed general.import while a root import exists.
        if doc.get("import").is_none()
            && doc
                .get("general")
                .is_some_and(|item| item.as_table_like().is_none())
        {
            return Err(invalid(path, "Alacritty 'general' must be a table"));
        }
        if let Some(item) = effective_import(&doc) {
            let array = item
                .as_array()
                .ok_or_else(|| invalid(path, "Alacritty import must be an array of paths"))?;
            if array.iter().any(|value| value.as_str().is_none()) {
                return Err(invalid(
                    path,
                    "Alacritty import must contain only path strings; repair it before applying",
                ));
            }
        }
        Ok(Self {
            path: path.to_owned(),
            original,
            doc,
        })
    }

    pub(super) fn prepare(mut self, paths: &[PathBuf], has_managed_font: bool) -> Result<Prepared> {
        let mut changed = false;
        let imports = if self.doc.get("import").is_some() {
            self.doc.get_mut("import").and_then(Item::as_array_mut)
        } else {
            let general = self
                .doc
                .entry("general")
                .or_insert(Item::Table(Table::new()))
                .as_table_like_mut()
                .ok_or_else(|| invalid(&self.path, "Alacritty 'general' must be a table"))?;
            general
                .entry("import")
                .or_insert(Item::Value(Value::Array(Array::new())))
                .as_array_mut()
        }
        .ok_or_else(|| invalid(&self.path, "Alacritty import must be an array of paths"))?;
        for path in paths {
            let managed = path
                .to_str()
                .ok_or_else(|| invalid(&self.path, "managed import path must be UTF-8"))?;
            if !imports.iter().any(|value| value.as_str() == Some(managed)) {
                imports.push(managed);
                changed = true;
            }
        }
        // Only the family supplied by this operation needs to stop shadowing
        // the import. Preserve style, size, bold/italic and all other settings.
        if has_managed_font {
            if let Some(font) = self.doc.get_mut("font") {
                let font = font
                    .as_table_like_mut()
                    .ok_or_else(|| invalid(&self.path, "Alacritty 'font' must be a table"))?;
                if let Some(normal) = font.get_mut("normal") {
                    let normal = normal.as_table_like_mut().ok_or_else(|| {
                        invalid(&self.path, "Alacritty 'font.normal' must be a table")
                    })?;
                    changed |= normal.remove("family").is_some();
                }
            }
        }
        let replacement = changed.then(|| self.doc.to_string().into_bytes());
        if replacement
            .as_ref()
            .is_some_and(|bytes| bytes.len() as u64 > MAX_TOOL_CONFIG_BYTES)
        {
            return Err(invalid(
                &self.path,
                "updated Alacritty config exceeds the 8 MiB limit",
            ));
        }
        Ok(Prepared {
            path: self.path,
            original: self.original,
            replacement,
        })
    }
}

/// Prepare from an already bounded, captured source without rereading the path.
pub(crate) fn font_content(path: &Path, original: &Source, managed: &Path) -> Result<Vec<u8>> {
    let prepared =
        Document::from_source(path, original.clone())?.prepare(&[managed.to_owned()], true)?;
    Ok(prepared.replacement.unwrap_or(prepared.original.bytes))
}

// No Debug/serialization: original and replacement may contain private config.
pub(super) struct Prepared {
    path: PathBuf,
    original: Source,
    replacement: Option<Vec<u8>>,
}

impl Prepared {
    pub(super) fn verify(&self) -> Result<()> {
        if source(&self.path)?.as_ref() != Some(&self.original) {
            return Err(invalid(
                &self.path,
                "Alacritty config changed while preparing; retry without overwriting the edit",
            ));
        }
        Ok(())
    }

    pub(super) fn publish(self) -> Result<()> {
        self.verify()?;
        if let Some(replacement) = self.replacement {
            if replacement != self.original.bytes {
                atomic_write_synced(&self.path, &replacement)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::{symlink, PermissionsExt};

    #[test]
    fn prepared_alacritty_output_limit_is_checked_before_writing() {
        let td = tempfile::tempdir().unwrap();
        let path = td.path().join("alacritty.toml");
        let original = format!("#{}\n", " ".repeat(MAX_TOOL_CONFIG_BYTES as usize - 2));
        fs::write(&path, &original).unwrap();
        let document = Document::read(&path).unwrap().unwrap();
        match document.prepare(&[td.path().join("colors.toml")], false) {
            Err(error) => assert!(error
                .to_string()
                .contains("updated Alacritty config exceeds")),
            Ok(_) => panic!("oversize replacement must be rejected before publication"),
        }
        assert_eq!(fs::read_to_string(&path).unwrap(), original);
    }

    #[test]
    fn prepared_color_only_edit_keeps_the_user_font_and_noop_bytes() {
        let td = tempfile::tempdir().unwrap();
        let path = td.path().join("alacritty.toml");
        let managed = td.path().join("colors.toml");
        let original = "[font.normal]\nfamily = 'User Mono'\nstyle = 'Medium'\n";
        fs::write(&path, original).unwrap();
        Document::read(&path)
            .unwrap()
            .unwrap()
            .prepare(std::slice::from_ref(&managed), false)
            .unwrap()
            .publish()
            .unwrap();
        let after = fs::read_to_string(&path).unwrap();
        let parsed: DocumentMut = after.parse().unwrap();
        assert_eq!(
            parsed["font"]["normal"]["family"].as_str(),
            Some("User Mono")
        );
        assert_eq!(parsed["font"]["normal"]["style"].as_str(), Some("Medium"));
        // An already connected CRLF document is returned without serialization.
        let crlf = after.replace('\n', "\r\n");
        fs::write(&path, &crlf).unwrap();
        let before = fs::metadata(&path).unwrap().modified().unwrap();
        Document::read(&path)
            .unwrap()
            .unwrap()
            .prepare(&[managed], false)
            .unwrap()
            .publish()
            .unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), crlf);
        assert_eq!(fs::metadata(&path).unwrap().modified().unwrap(), before);
    }

    #[test]
    fn prepared_alacritty_edit_rejects_changed_source_before_publication() {
        for mutation in ["bytes", "identity", "mode", "missing", "link"] {
            let td = tempfile::tempdir().unwrap();
            let path = td.path().join("alacritty.toml");
            let original = "import = ['user.toml']\n";
            fs::write(&path, original).unwrap();
            fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();
            let prepared = Document::read(&path)
                .unwrap()
                .unwrap()
                .prepare(&[td.path().join("colors.toml")], false)
                .unwrap();
            prepared.verify().unwrap();
            match mutation {
                "bytes" => fs::write(&path, "# PRIVATE_CONTENT_CHANGED\n").unwrap(),
                "identity" => {
                    let replacement = td.path().join("replacement");
                    fs::write(&replacement, original).unwrap();
                    fs::set_permissions(&replacement, fs::Permissions::from_mode(0o640)).unwrap();
                    fs::rename(replacement, &path).unwrap();
                }
                "mode" => fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap(),
                "missing" => fs::remove_file(&path).unwrap(),
                "link" => {
                    let target = td.path().join("target");
                    fs::rename(&path, &target).unwrap();
                    symlink(target, &path).unwrap();
                }
                _ => unreachable!(),
            }
            let before = fs::read(&path).ok();
            let error = prepared.publish().unwrap_err().to_string();
            assert!(!error.contains("PRIVATE_CONTENT_CHANGED"));
            assert_eq!(fs::read(&path).ok(), before);
            if mutation == "mode" {
                assert_eq!(
                    fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                    0o600
                );
            }
            if mutation == "link" {
                assert!(fs::symlink_metadata(&path).unwrap().is_symlink());
            }
        }
    }
}
