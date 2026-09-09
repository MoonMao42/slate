//! Bounded, best-effort literal include graph; never a Ghostty syntax validator.
use super::ghostty_references::{self, Directive, Reference};
use super::ghostty_window_style::{self, Assignment};
use crate::config::file_read::{self, Links, MAX_TOOL_CONFIG_BYTES};
use crate::env::SlateEnv;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

const MAX_FILES: usize = 256;
const MAX_BYTES: u64 = 32 * 1024 * 1024;
const MAX_EDGES: usize = 4096;
const MAX_DEPTH: usize = 64;

fn scan_path(path: &Path) -> PathBuf {
    if let Ok(canonical) = std::fs::canonicalize(path) {
        return canonical;
    }
    // Even absent suffixes need their existing parent aliases resolved (notably
    // /var -> /private/var on macOS), before testing isolated-profile containment.
    // Keep unresolved suffixes intact: lexical removal of '..' could otherwise
    // read a different file through a broken absolute-path prefix.
    file_read::directory_alias_target(path).unwrap_or_else(|| path.to_owned())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(super) struct Issue {
    pub code: &'static str,
    pub path: String,
    pub path_is_lossy: bool,
    pub message: String,
}

#[derive(Default)]
struct Node {
    slate_refs: Vec<String>,
    includes: Vec<Reference>,
    missing: bool,
}

pub(super) struct Scan<'a> {
    env: &'a SlateEnv,
    managed_theme: PathBuf,
    nodes: BTreeMap<PathBuf, Node>,
    bytes: u64,
    edges: usize,
    exhausted: bool,
    reported_missing: BTreeSet<PathBuf>,
    pub issues: Vec<Issue>,
    pub cycles: Vec<Vec<PathBuf>>,
    pub window_style_overrides: BTreeMap<PathBuf, Assignment>,
}

impl<'a> Scan<'a> {
    pub fn new(env: &'a SlateEnv) -> Self {
        let managed = env.managed_file("managed/ghostty/theme.conf");
        Self {
            env,
            // Resolve directory aliases but not a final file link: putting a
            // symlink here must not relabel user settings elsewhere as Slate's.
            managed_theme: file_read::directory_alias_target(&managed).unwrap_or(managed),
            nodes: BTreeMap::new(),
            bytes: 0,
            edges: 0,
            exhausted: false,
            reported_missing: BTreeSet::new(),
            issues: Vec::new(),
            cycles: Vec::new(),
            window_style_overrides: BTreeMap::new(),
        }
    }

    fn issue(&mut self, code: &'static str, path: &Path, message: impl Into<String>) {
        self.issues.push(Issue {
            code,
            path: path.display().to_string(),
            path_is_lossy: path.to_str().is_none(),
            message: message.into(),
        });
    }

    fn load(&mut self, path: &Path) -> PathBuf {
        // Cache by load spelling, not physical file: absolute file aliases can
        // give identical bytes a different base for nested relative references.
        let path = path.to_owned();
        if self.nodes.contains_key(&path) || self.exhausted {
            return path;
        }
        if self.nodes.len() >= MAX_FILES {
            self.issue(
                "file_limit",
                &path,
                "Configuration scan reached its 256-file limit",
            );
            self.exhausted = true;
            return path;
        }
        // Cache failures/absence too, so repeated edges cannot repeat IO or issues.
        self.nodes.insert(path.clone(), Node::default());
        if self.env.session().is_isolated()
            && !scan_path(&path).starts_with(scan_path(self.env.home()))
        {
            self.issue(
                "outside_profile",
                &path,
                "Reference escapes the isolated SLATE_HOME; contents not read",
            );
            return path;
        }
        let remaining = MAX_BYTES.saturating_sub(self.bytes);
        if remaining == 0 {
            self.issue(
                "byte_limit",
                &path,
                "Configuration scan reached its 32 MiB total read limit",
            );
            self.exhausted = true;
            return path;
        }
        let source =
            match file_read::read(&path, MAX_TOOL_CONFIG_BYTES.min(remaining), Links::Follow) {
                Ok(Some(source)) => source,
                Ok(None) => {
                    self.nodes.get_mut(&path).expect("inserted node").missing = true;
                    return path;
                }
                Err(file_read::ReadError::TooLarge(_)) if remaining < MAX_TOOL_CONFIG_BYTES => {
                    self.issue(
                        "byte_limit",
                        &path,
                        "File exceeds the remaining 32 MiB scan budget; skipped",
                    );
                    return path;
                }
                Err(error) => {
                    self.issue(
                        "read_error",
                        &path,
                        format!("Cannot safely read configuration: {error}"),
                    );
                    return path;
                }
            };
        self.bytes += source.bytes.len() as u64;
        let Ok(content) = std::str::from_utf8(&source.bytes) else {
            self.issue(
                "read_error",
                &path,
                "Expected UTF-8 configuration; contents omitted",
            );
            return path;
        };
        let managed = self.env.managed_file("managed/ghostty");
        // Identity is diagnostic evidence only, never authorization to overwrite
        // a linked file. Reuse this scan's reads and limits; do not read an
        // unreferenced generated file just to look for an old style override.
        let identity = scan_path(&path);
        let is_managed_theme = identity == self.managed_theme;
        let parent = path.parent().unwrap_or_else(|| Path::new("/"));
        let mut node = Node::default();
        for (index, line) in content
            .strip_prefix('\u{feff}')
            .unwrap_or(content)
            .split('\n')
            .enumerate()
        {
            if line.len() > 4094 {
                self.issue("line_limit", &path, "Line exceeds Ghostty 1.3.1's 4094-byte input limit; remaining lines not inspected");
                break;
            }
            if is_managed_theme && ghostty_window_style::is_assignment(line) {
                self.window_style_overrides
                    .entry(identity.clone())
                    .or_insert_with(|| Assignment::new(&path, index + 1));
            }
            let (value, optional, legacy) = match ghostty_references::parse(line) {
                Directive::Ignore => continue,
                Directive::Invalid => {
                    self.issue(
                        "invalid_reference",
                        &path,
                        "Cannot inspect config-file directive; value omitted",
                    );
                    continue;
                }
                Directive::Reset => {
                    node.includes.clear();
                    node.slate_refs.clear();
                    self.issue("reference_reset", &path, "config-file list reset discards earlier local references; cross-file reset order is not modeled");
                    continue;
                }
                Directive::Path {
                    value,
                    optional,
                    legacy,
                } => (value, optional, legacy),
            };
            if self.edges >= MAX_EDGES {
                self.issue(
                    "edge_limit",
                    &path,
                    "Configuration scan reached its 4096-reference limit",
                );
                self.exhausted = true;
                break;
            }
            self.edges += 1;
            node.slate_refs
                .extend(ghostty_references::managed(value, &managed));
            // Retain historical Slate-hook evidence for `include`, but it is
            // not a Ghostty config-file edge and must not be recursively loaded.
            if !legacy {
                match ghostty_references::resolve(value, parent, self.env.home()) {
                    Ok(path) => node.includes.push(Reference { path, optional }),
                    Err(_) => self.issue(
                        "unresolved_reference",
                        &path,
                        "Cannot resolve a relative config-file reference; value omitted",
                    ),
                }
            }
        }
        self.nodes.insert(path.clone(), node);
        path
    }

    pub fn slate_refs(&mut self, path: &Path) -> Vec<String> {
        let path = self.load(path);
        self.nodes
            .get(&path)
            .map(|node| node.slate_refs.clone())
            .unwrap_or_default()
    }

    pub fn visit_all(&mut self, paths: impl IntoIterator<Item = PathBuf>) {
        let mut completed = BTreeSet::new();
        let mut stack = Vec::new();
        let mut emitted = BTreeSet::new();
        for path in paths {
            self.visit(
                &Reference {
                    path,
                    optional: true,
                },
                &mut stack,
                &mut completed,
                &mut emitted,
            );
        }
    }

    fn visit(
        &mut self,
        reference: &Reference,
        stack: &mut Vec<PathBuf>,
        completed: &mut BTreeSet<PathBuf>,
        emitted: &mut BTreeSet<Vec<PathBuf>>,
    ) {
        let path = &reference.path;
        let identity = scan_path(path);
        if let Some(pos) = stack.iter().position(|entry| entry == &identity) {
            let mut cycle = stack[pos..].to_vec();
            cycle.push(identity);
            if emitted.insert(cycle.clone()) {
                self.cycles.push(cycle);
            }
            return;
        }
        if self.exhausted {
            return;
        }
        if stack.len() >= MAX_DEPTH {
            self.issue(
                "depth_limit",
                path,
                "Configuration scan reached its 64-level include limit",
            );
            // Do not mark completed: a later shallower entry may still be scanned.
            return;
        }
        let path = self.load(path);
        if !reference.optional
            && self.nodes.get(&path).is_some_and(|node| node.missing)
            && self.reported_missing.insert(path.clone())
        {
            self.issue(
                "required_file_missing",
                &path,
                "Required config-file reference is missing; only a '?' reference permits absence",
            );
        }
        if completed.contains(&path) {
            return;
        }
        let includes = self
            .nodes
            .get(&path)
            .map(|node| node.includes.clone())
            .unwrap_or_default();
        stack.push(identity);
        for include in includes {
            self.visit(&include, stack, completed, emitted);
        }
        stack.pop();
        completed.insert(path);
    }
}
