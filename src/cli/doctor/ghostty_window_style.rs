//! Advisory evidence from the existing bounded include scan, never live UI state.
use serde::Serialize;
use std::path::Path;

pub(super) const SCOPE: &str = "Literal macos-titlebar-style assignments in referenced regular Slate managed theme files, not proof of the effective style or live appearance. A managed-file symlink does not make its target Slate-owned. User-owned settings elsewhere are not warnings. Unreferenced files and contents beyond scan limits are not inspected; assignment values are omitted here.";
const NEXT_STEP: &str = "Reapply the desired theme using this version of Slate to regenerate theme.conf. Keep macos-titlebar-style in your own Ghostty config, not the generated theme file. Reload and compare a new macOS window; native tab colors still need visual checking.";
const CHECK_APPEARANCE: &str = "No Slate-managed titlebar assignment was found, but this does not verify native tab colors or a running window. If the top bar still looks different, compare a newly opened window and inspect your own titlebar/window-theme and opacity settings before changing them. A successful config validation is not a visual match.";
const COMPLETE_SCAN: &str = "Finish the incomplete configuration scan before concluding that no managed titlebar override exists. Review the reported scan issues; no automatic configuration change or appearance fix is implied.";

fn next_step(found: &[Assignment], complete: bool) -> &'static str {
    if !found.is_empty() {
        NEXT_STEP
    } else if complete {
        CHECK_APPEARANCE
    } else {
        COMPLETE_SCAN
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(super) struct Assignment {
    pub path: String,
    pub path_is_lossy: bool,
    pub first_assignment_line: usize,
}

impl Assignment {
    pub fn new(path: &Path, first_assignment_line: usize) -> Self {
        Self {
            path: path.display().to_string(),
            path_is_lossy: path.to_str().is_none(),
            first_assignment_line,
        }
    }
}

/// Follow the line reader's ASCII whitespace and exact-key rules. Deliberately
/// do not interpret or display values: an assignment is not an effective setting
/// or syntax verdict. BOM and native line limits are handled by the caller.
pub(super) fn is_assignment(line: &str) -> bool {
    line.trim_matches([' ', '\t', '\r'])
        .split_once('=')
        .is_some_and(|(key, _)| key.trim_matches([' ', '\t']) == "macos-titlebar-style")
}

fn status(found: &[Assignment], complete: bool) -> &'static str {
    if !found.is_empty() {
        "managed_override"
    } else if complete {
        "not_found"
    } else {
        "unknown"
    }
}

pub(super) fn json(found: &[Assignment], complete: bool) -> serde_json::Value {
    serde_json::json!({
        "applies_to": "macos",
        "status": status(found, complete),
        "inspection_complete": complete,
        "managed_overrides": found,
        "next_step": next_step(found, complete),
        "scope": SCOPE,
    })
}

pub(super) fn format(found: &[Assignment], complete: bool) -> String {
    let mut text = match status(found, complete) {
        "managed_override" => "window layout: managed override found (macOS)\n".to_owned(),
        "not_found" => "window layout: no managed override found in inspected files\n".to_owned(),
        _ => "window layout: unknown; configuration scan is incomplete\n".to_owned(),
    };
    for assignment in found {
        text.push_str(&format!(
            "  - {}{}: first assignment at line {}\n",
            super::integrations::terminal_text(&assignment.path),
            if assignment.path_is_lossy {
                " (lossy display; not an exact path)"
            } else {
                ""
            },
            assignment.first_assignment_line
        ));
    }
    text.push_str(next_step(found, complete));
    text.push('\n');
    text.push_str(SCOPE);
    text.push('\n');
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn titlebar_guidance_distinguishes_missing_evidence_from_visual_success() {
        for (found, complete, expected, guidance) in [
            (vec![], true, "not_found", CHECK_APPEARANCE),
            (vec![], false, "unknown", COMPLETE_SCAN),
            (
                vec![Assignment::new(Path::new("/fixture/theme.conf"), 3)],
                true,
                "managed_override",
                NEXT_STEP,
            ),
            (
                vec![Assignment::new(Path::new("/fixture/theme.conf"), 3)],
                false,
                "managed_override",
                NEXT_STEP,
            ),
        ] {
            let report = json(&found, complete);
            assert_eq!(report["status"], expected);
            assert_eq!(report["next_step"], guidance);
            assert!(format(&found, complete).contains(guidance));
        }
    }
}
