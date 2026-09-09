use super::choices::{terminal_text, Choices, Kind, Presence};
use crate::{
    adapter::font::{FontAdapter, FontScanReport},
    env::SlateEnv,
    error::Result,
    platform::fonts,
};
use serde::Serialize;
use std::{io::Write, path::Path};

const SCOPE: &str = "Read-only filename/signature candidates and the embedded download catalog, not all installed fonts. System names use a platform whitelist; ordinary font links and standard system roots are scanned even in an isolated profile. Candidate matches use normalized names, not internal names, native registration or glyph coverage. Partial scans cannot establish absence; catalog download offers are withheld for unknown entries. Listing does not read saved settings, select a font, launch a tool, install or refresh caches. Network access, write readiness and terminal appearance are unverified. Search only filters displayed choices after the full scan; no search matches does not establish absence. Check scan_complete and scan_issues before relying on negative results.";

#[derive(Serialize)]
pub(super) struct PathView {
    pub path: String,
    pub path_is_lossy: bool,
}
impl From<&Path> for PathView {
    fn from(path: &Path) -> Self {
        Self {
            path: path.display().to_string(),
            path_is_lossy: path.to_str().is_none(),
        }
    }
}
#[derive(Serialize)]
pub(super) struct Issue {
    #[serde(flatten)]
    pub path: PathView,
    pub reason: &'static str,
}
impl From<&crate::adapter::font::FontScanIssue> for Issue {
    fn from(issue: &crate::adapter::font::FontScanIssue) -> Self {
        Self {
            path: issue.path.as_path().into(),
            reason: issue.reason,
        }
    }
}
#[derive(Serialize)]
struct Listing {
    schema_version: u8,
    backend: &'static str,
    scope: &'static str,
    scan_complete: bool,
    #[serde(flatten)]
    choices: Choices,
    search_roots: Vec<PathView>,
    scan_issues: Vec<Issue>,
    omitted_issue_count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    search: Option<SearchSummary>,
}

#[derive(Serialize)]
struct SearchSummary {
    query: String,
    total_candidates: usize,
    matched_candidates: usize,
    total_catalog_entries: usize,
    matched_catalog_entries: usize,
}

/// Keep validation settings-free and repeat it for non-CLI callers, before any
/// discovery. Search text is data; escaped presentation handles its controls.
pub fn validate_list_query(query: &str) -> Result<()> {
    if query.len() > 256 {
        return Err(crate::error::SlateError::InvalidConfig(
            "Font list search must be at most 256 bytes.".into(),
        ));
    }
    Ok(())
}

fn filter_choices(choices: &mut Choices, query: &str) -> SearchSummary {
    let mut terms: Vec<_> = query
        .split(|c: char| !c.is_alphanumeric())
        .filter(|term| !term.is_empty())
        .map(FontAdapter::family_match_key)
        .collect();
    terms.sort_unstable();
    terms.dedup();
    let match_all = query.trim().is_empty();
    let matches = |fields: &[&str]| {
        match_all
            || (!terms.is_empty() && {
                let keys: Vec<_> = fields
                    .iter()
                    .map(|field| FontAdapter::family_match_key(field))
                    .collect();
                terms
                    .iter()
                    .all(|term| keys.iter().any(|key| key.contains(term)))
            })
    };
    let total_candidates = choices.candidates.len();
    let total_catalog_entries = choices.catalog.len();
    choices
        .candidates
        .retain(|candidate| matches(&[&candidate.family]));
    choices
        .catalog
        .retain(|entry| matches(&[entry.id, entry.family]));
    SearchSummary {
        query: query.to_owned(),
        total_candidates,
        matched_candidates: choices.candidates.len(),
        total_catalog_entries,
        matched_catalog_entries: choices.catalog.len(),
    }
}

fn report(env: &SlateEnv, scan: &FontScanReport, query: Option<&str>) -> Listing {
    // Presence and download consent are computed from the FULL scan before
    // filtering. Retain matching_candidates evidence even if hidden by a query.
    let mut choices = Choices::from_scan(scan);
    let search = query.map(|query| filter_choices(&mut choices, query));
    Listing {
        schema_version: 1,
        backend: fonts::backend().label(),
        scope: SCOPE,
        scan_complete: scan.is_complete(),
        choices,
        search_roots: fonts::font_search_paths(env)
            .iter()
            .map(|path| PathView::from(path.as_path()))
            .collect(),
        scan_issues: scan.issues.iter().map(Issue::from).collect(),
        omitted_issue_count: scan.omitted_issues,
        search,
    }
}

fn text_report(report: &Listing) -> String {
    use std::fmt::Write;
    let mut output =
        String::from("Font candidates (filename/header evidence, not native activation)\n");
    if let Some(search) = &report.search {
        let _ = writeln!(output, "Search {:?}: {}/{} candidates, {}/{} catalog choices. Filtering does not establish availability.", search.query, search.matched_candidates, search.total_candidates, search.matched_catalog_entries, search.total_catalog_entries);
    }
    if report.choices.candidates.is_empty() {
        output.push_str(if report.search.is_some() {
            "  No candidates match this search; this does not mean the font is absent.\n"
        } else {
            "  No candidates observed in this scan.\n"
        });
    }
    for candidate in &report.choices.candidates {
        let kind = if candidate.kind == Kind::Nerd {
            "nerd"
        } else {
            "system"
        };
        let _ = writeln!(
            output,
            "  {} [{kind}]{}",
            terminal_text(&candidate.family),
            if candidate.recommended {
                " (recommended)"
            } else {
                ""
            }
        );
    }
    output.push_str("\nCatalog (listing does not download anything)\n");
    if report.search.is_some() && report.choices.catalog.is_empty() {
        output.push_str("  No catalog choices match this search.\n");
    }
    for entry in &report.choices.catalog {
        let status = match entry.presence {
            Presence::CandidateFound => "candidate found; choose an exact observed family",
            Presence::NotObserved => "not observed; download offered on selection",
            Presence::Unknown => "unknown; download withheld until scan issues are resolved",
        };
        let _ = writeln!(output, "  {} — {} [{status}]", entry.id, entry.family);
    }
    let _ = writeln!(
        output,
        "\nScan: {}. {} issue(s) reported, {} more omitted.",
        if report.scan_complete {
            "complete within supported search"
        } else {
            "incomplete"
        },
        report.scan_issues.len(),
        report.omitted_issue_count
    );
    for issue in &report.scan_issues {
        let _ = writeln!(
            output,
            "  {}: {}{}",
            issue.reason,
            terminal_text(&issue.path.path),
            if issue.path.path_is_lossy {
                " (lossy path display)"
            } else {
                ""
            }
        );
    }
    output.push_str("\nChoose with `slate font -- '<exact family>'`; catalog IDs may trigger installation. Use `slate doctor font` for saved settings and generated-file diagnostics.\n");
    let _ = writeln!(output, "{}", report.scope);
    output
}

/// Lists observations only, including partial results. Exit success means report
/// production; machine callers must inspect scan_complete before using absence.
pub fn handle_list(env: &SlateEnv, json: bool) -> Result<()> {
    handle_list_with_query(env, json, None)
}

/// Search only changes the view; scan evidence and mutation gates are unchanged.
pub fn handle_list_with_query(env: &SlateEnv, json: bool, query: Option<&str>) -> Result<()> {
    if let Some(query) = query {
        validate_list_query(query)?;
    }
    let report = report(env, &FontAdapter::scan_fonts_with_env(env), query);
    let output = if json {
        format!("{}\n", serde_json::to_string_pretty(&report)?)
    } else {
        text_report(&report)
    };
    match std::io::stdout().lock().write_all(output.as_bytes()) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::BrokenPipe => Ok(()),
        Err(error) => Err(error.into()),
    }
}

#[cfg(test)]
#[path = "listing_search_tests.rs"]
mod search_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::ffi::OsStringExt;

    #[test]
    fn font_list_json_preserves_partial_counts_and_discloses_lossy_paths() {
        // Serialization only; no non-UTF-8 filename is created on APFS.
        let path = std::path::PathBuf::from(std::ffi::OsString::from_vec(
            b"/private-font-list/\xff\x1b[2J".to_vec(),
        ));
        let env = SlateEnv::with_home(path.clone());
        let scan = FontScanReport {
            issues: vec![crate::adapter::font::FontScanIssue {
                path,
                reason: "cannot inspect font directory",
            }],
            omitted_issues: 9,
            ..Default::default()
        };
        let report = report(&env, &scan, None);
        let json = serde_json::to_value(&report).unwrap();
        assert!(json.get("search").is_none());
        assert_eq!(json["scan_complete"], false);
        assert_eq!(json["omitted_issue_count"], 9);
        assert_eq!(json["search_roots"][0]["path_is_lossy"], true);
        assert_eq!(json["scan_issues"][0]["path_is_lossy"], true);
        assert!(json["catalog"]
            .as_array()
            .unwrap()
            .iter()
            .all(|entry| entry["presence"] == "unknown" && entry["download_offered"] == false));
        assert!(!text_report(&report).contains('\u{1b}'));
    }
}
