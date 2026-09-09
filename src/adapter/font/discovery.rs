//! Read-only, bounded discovery of filename-derived font candidates. A small
//! SFNT signature check rejects obvious impostors; this is not font validation,
//! native registration, name-table parsing or proof of Nerd Font glyph coverage.
use super::{FontAdapter, FontDiscovery};
use crate::{
    env::SlateEnv,
    error::{Result, SlateError},
};
use std::{
    collections::BTreeSet,
    fs::{self, Metadata, OpenOptions},
    io::Read,
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
};

const MAX_ISSUES: usize = 32;
#[derive(Clone, Copy)]
struct Limits {
    entries: usize,
    directories: usize,
    depth: usize,
}
const LIMITS: Limits = Limits {
    entries: 50_000,
    directories: 4096,
    depth: 16,
};

#[derive(Debug, Clone)]
pub struct FontScanIssue {
    pub path: PathBuf,
    pub reason: &'static str,
}

#[derive(Debug, Default)]
pub struct FontScanReport {
    pub fonts: FontDiscovery,
    pub issues: Vec<FontScanIssue>,
    pub omitted_issues: usize,
}

impl FontScanReport {
    pub fn is_complete(&self) -> bool {
        self.issues.is_empty() && self.omitted_issues == 0
    }

    fn issue(&mut self, path: &Path, reason: &'static str) {
        if self.issues.len() < MAX_ISSUES {
            self.issues.push(FontScanIssue {
                path: path.to_owned(),
                reason,
            });
        } else {
            self.omitted_issues += 1;
        }
    }

    pub fn warning(&self) -> String {
        self.warning_in(crate::config::ui_language::UiLanguage::English)
    }

    pub fn warning_in(&self, language: crate::config::ui_language::UiLanguage) -> String {
        let details = self
            .issues
            .iter()
            .take(3)
            .map(|issue| {
                let path: String = issue
                    .path
                    .display()
                    .to_string()
                    .escape_default()
                    .take(240)
                    .collect();
                format!("{} ({})", issue.reason, path)
            })
            .collect::<Vec<_>>()
            .join("; ");
        if language == crate::config::ui_language::UiLanguage::Chinese {
            return format!("字体扫描未完成（{} 个问题）：{}。已找到的候选仍可使用；请先检查字体目录权限和扫描限制，不要据此认定字体缺失。", self.issues.len() + self.omitted_issues, details);
        }
        format!("Font discovery is incomplete ({} issue(s)): {}. Known candidates remain usable; check font-directory access/scan limits before assuming a font is absent.", self.issues.len() + self.omitted_issues, details)
    }

    pub fn require_complete(&self) -> Result<()> {
        if self.is_complete() {
            Ok(())
        } else {
            Err(SlateError::Internal(self.warning()))
        }
    }

    /// A positive observation is useful even in a partial scan. Absence is not.
    pub(crate) fn contains_nerd_family(&self, name: &str) -> Result<bool> {
        let key = FontAdapter::family_match_key(name);
        if self
            .fonts
            .nerd_fonts
            .iter()
            .any(|family| FontAdapter::family_match_key(family) == key)
        {
            return Ok(true);
        }
        self.require_complete()?;
        Ok(false)
    }
}

pub(super) fn scan(env: &SlateEnv) -> FontScanReport {
    let whitelist = if cfg!(target_os = "macos") {
        &["Monaco", "Menlo", "SF Mono"][..]
    } else {
        &["DejaVu Sans Mono", "Liberation Mono", "Ubuntu Mono"][..]
    };
    scan_roots(
        &crate::platform::fonts::font_search_paths(env),
        whitelist,
        LIMITS,
    )
}

fn identity(meta: &Metadata) -> (u64, u64) {
    (meta.dev(), meta.ino())
}
fn same_file(a: &Metadata, b: &Metadata) -> bool {
    b.is_file()
        && identity(a) == identity(b)
        && a.len() == b.len()
        && (a.mtime(), a.mtime_nsec(), a.ctime(), a.ctime_nsec())
            == (b.mtime(), b.mtime_nsec(), b.ctime(), b.ctime_nsec())
}

fn has_header(path: &Path, before: &Metadata) -> std::result::Result<bool, &'static str> {
    if before.len() <= 12 {
        return Ok(false);
    }
    // Follow ordinary font links, but inspect the opened type before reading.
    // O_NONBLOCK prevents a replacement FIFO from blocking the read-only scan.
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK | libc::O_CLOEXEC)
        .open(path)
        .map_err(|_| "cannot open font candidate")?;
    let opened = file.metadata().map_err(|_| "cannot inspect opened font")?;
    if !same_file(before, &opened) {
        return Err("font candidate changed before reading");
    }
    let mut prefix = [0; 12];
    file.read_exact(&mut prefix)
        .map_err(|_| "cannot read font header")?;
    let after = file.metadata().map_err(|_| "cannot inspect read font")?;
    let current = fs::metadata(path).map_err(|_| "font candidate moved while reading")?;
    if !same_file(&opened, &after) || !same_file(&opened, &current) {
        return Err("font candidate changed while reading");
    }
    Ok(crate::platform::fonts::has_sfnt_signature(&prefix))
}

fn scan_roots(roots: &[PathBuf], whitelist: &[&str], limits: Limits) -> FontScanReport {
    let mut report = FontScanReport::default();
    let mut nerd = BTreeSet::new();
    let mut system = BTreeSet::new();
    let mut seen = BTreeSet::new();
    let mut pending: Vec<_> = roots.iter().rev().map(|path| (path.clone(), 0)).collect();
    let mut count = 0;
    while let Some((directory, depth)) = pending.pop() {
        let before = match fs::metadata(&directory) {
            Ok(meta) if meta.is_dir() => meta,
            Ok(_) => {
                report.issue(&directory, "font search root is not a directory");
                continue;
            }
            Err(error) => {
                // A genuinely absent optional root is normal. A dangling link,
                // unreadable root or a disappeared queued child is not absence.
                let absent_root = depth == 0
                    && error.kind() == std::io::ErrorKind::NotFound
                    && crate::config::file_read::confirm_missing(&directory).is_ok();
                if !absent_root {
                    report.issue(&directory, "cannot inspect font directory");
                }
                continue;
            }
        };
        if seen.contains(&identity(&before)) {
            continue;
        }
        if depth > limits.depth {
            report.issue(&directory, "font directory depth limit reached");
            continue;
        }
        if seen.len() >= limits.directories {
            report.issue(&directory, "font directory count limit reached");
            break;
        }
        seen.insert(identity(&before));
        let entries = match fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(_) => {
                report.issue(&directory, "cannot list font directory");
                continue;
            }
        };
        let mut paths = Vec::new();
        let mut exhausted = false;
        for entry in entries {
            if count >= limits.entries {
                report.issue(&directory, "font entry count limit reached");
                exhausted = true;
                break;
            }
            count += 1;
            match entry {
                Ok(entry) => paths.push(entry.path()),
                Err(_) => report.issue(&directory, "cannot inspect directory entry"),
            }
        }
        paths.sort();
        let mut children = Vec::new();
        for path in paths {
            let meta = match fs::metadata(&path) {
                Ok(meta) => meta,
                Err(_) => {
                    report.issue(&path, "cannot inspect font entry or link target");
                    continue;
                }
            };
            if meta.is_dir() {
                children.push((path, depth + 1));
                continue;
            }
            if !meta.is_file() || !crate::platform::fonts::supported_font_extension(&path) {
                continue;
            }
            let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                report.issue(&path, "font filename is not UTF-8");
                continue;
            };
            if name
                .chars()
                .any(|c| c.is_control() || matches!(c, '\u{2028}' | '\u{2029}'))
            {
                continue;
            }
            let family = FontAdapter::normalize_font_family(name);
            if crate::adapter::font_config::validate_family(&family).is_err() {
                continue;
            }
            let is_nerd = FontAdapter::is_nerd_font_name(&family);
            let system_name = whitelist.iter().find(|candidate| {
                FontAdapter::family_match_key(candidate) == FontAdapter::family_match_key(&family)
            });
            if !is_nerd && system_name.is_none() {
                continue;
            }
            match has_header(&path, &meta) {
                Ok(true) => {
                    if is_nerd {
                        nerd.insert(family);
                    }
                    if let Some(name) = system_name {
                        system.insert((*name).to_owned());
                    }
                }
                Ok(false) => {} // Known non-font content, not an unreadable result.
                Err(reason) => report.issue(&path, reason),
            }
        }
        if !fs::metadata(&directory).is_ok_and(|after| {
            after.is_dir()
                && identity(&after) == identity(&before)
                && (after.mtime(), after.mtime_nsec()) == (before.mtime(), before.mtime_nsec())
        }) {
            report.issue(&directory, "font directory changed while scanning");
        }
        if exhausted {
            break;
        }
        pending.extend(children.into_iter().rev());
    }
    report.fonts = FontDiscovery {
        nerd_fonts: nerd.into_iter().collect(),
        system_fonts: system.into_iter().collect(),
    };
    report
}

#[cfg(test)]
#[path = "discovery_tests.rs"]
mod tests;
