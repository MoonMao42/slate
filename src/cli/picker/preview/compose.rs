//! Responsive fold composer for picker full-preview mode.
//!
//! Decides how many blocks to stack (4 / 6 / 8) based on terminal rows
//! and assembles them with ◆ Heading labels per sketch 005 A.
//!
//! Filled in Plan 19-04 (Wave 2).
//!
//! PHASE-19 WAVE-2 PARALLEL-WORKTREE STUB
//! --------------------------------------
//! Plan 19-05 (render mode dispatch) runs in a Wave-2 worktree *concurrently*
//! with Plan 19-04 (which owns the real `FoldTier` / `decide_fold_tier` /
//! `compose_full` implementation). 19-05's `render_full_preview` MUST call
//! `compose::compose_full` per its plan; to keep the 19-05 worktree
//! compilable + testable **before** Plan 19-04 merges, a minimum-shim
//! `FoldTier` + `decide_fold_tier` + `compose_full` live here. Signatures
//! match the 19-04 plan frontmatter `<interfaces>` block verbatim; 19-04's
//! merge will replace this whole module body with the real composer.
//!
//! Scope: tests + render.rs smoke — the shim output is intentionally
//! minimal (emits `◆ Palette`, `◆ Prompt`, `◆ Code`, `◆ Files` headings
//! at Minimum tier; adds Git/Diff at Medium; Lazygit/Nvim at Large) so
//! Plan 19-05's `mode_dispatch_uses_preview_mode_full` behavior contract
//! is satisfied without the 19-04 surface.
//!
//! MERGE-HANDOFF: when Plan 19-04 merges, this module body is replaced
//! wholesale by the real composer. The `pub(crate)` + `pub(super)` surface
//! is intentionally permissive here to survive either visibility choice
//! in the real impl; merge resolution should drop this shim in favor of
//! the 19-04 version.

use crate::brand::roles::Roles;
use crate::theme::Palette;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FoldTier {
    Minimum, // 4 blocks: Palette / Prompt / Code / Files
    Medium,  // 6 blocks: + Git / Diff
    Large,   // 8 blocks: + Lazygit / Nvim
}

/// D-13 fold thresholds — `rows < 32` → Minimum, `32..=39` → Medium,
/// `≥40` → Large. Matches the 19-04 plan interfaces block verbatim so the
/// real composer can drop in without renderer churn.
pub(crate) fn decide_fold_tier(rows: u16) -> FoldTier {
    match rows {
        0..=31 => FoldTier::Minimum,
        32..=39 => FoldTier::Medium,
        _ => FoldTier::Large,
    }
}

/// Minimum-shim composer (see module docs). Emits the `◆ Heading` labels
/// the Plan 19-05 `mode_dispatch_uses_preview_mode_full` test looks for.
/// Body text under each heading is deliberately terse (single placeholder
/// line) — Plan 19-04 supplies the real block bodies.
pub(crate) fn compose_full(
    _palette: &Palette,
    tier: FoldTier,
    roles: Option<&Roles<'_>>,
    prompt_line_override: Option<&str>,
) -> String {
    let mut out = String::with_capacity(512);
    push_heading(&mut out, roles, "Palette");
    out.push_str("(palette swatch placeholder — Plan 19-04 will supply)\n");

    push_heading(&mut out, roles, "Prompt");
    match prompt_line_override {
        Some(line) => {
            out.push_str(line);
            if !line.ends_with('\n') {
                out.push('\n');
            }
        }
        None => out.push_str("(prompt placeholder — Plan 19-04 will supply)\n"),
    }

    push_heading(&mut out, roles, "Code");
    out.push_str("(code block placeholder — Plan 19-04 will supply)\n");

    push_heading(&mut out, roles, "Files");
    out.push_str("(tree block placeholder — Plan 19-04 will supply)\n");

    if matches!(tier, FoldTier::Medium | FoldTier::Large) {
        push_heading(&mut out, roles, "Git");
        out.push_str("(git log placeholder — Plan 19-04 will supply)\n");
        push_heading(&mut out, roles, "Diff");
        out.push_str("(diff placeholder — Plan 19-04 will supply)\n");
    }
    if matches!(tier, FoldTier::Large) {
        push_heading(&mut out, roles, "Lazygit");
        out.push_str("(lazygit placeholder — Plan 19-04 will supply)\n");
        push_heading(&mut out, roles, "Nvim");
        out.push_str("(nvim placeholder — Plan 19-04 will supply)\n");
    }
    out
}

fn push_heading(out: &mut String, roles: Option<&Roles<'_>>, title: &str) {
    let heading = match roles {
        Some(r) => r.heading(title),
        None => format!("◆ {}", title),
    };
    out.push_str(&heading);
    out.push('\n');
}

#[cfg(test)]
mod tests {
    // Populated by Plan 19-04 (Wave 2) — this shim is overwritten on merge.
}
