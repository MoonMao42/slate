//! Canonical nvim highlight-group table. Consumed by
//! `src/adapter/nvim.rs::render_colorscheme` (Plan 02).
//!
//! Coverage (this file, Plan 01):
//!   • Base UI (~80)        — Normal, Comment, Pmenu, StatusLine, …
//!   • Diff/diagnostics (~40) — DiffAdd, DiagnosticError, LspReferenceText, …
//!   • Treesitter (~100)    — @function, @keyword.return, @string.regex, …
//!   • LSP semantic tokens (~42) — @lsp.type.parameter, @lsp.typemod.*, …
//!
//! Plugin groups (~130 entries for telescope / neo-tree / GitSigns /
//! which-key / blink.cmp / nvim-cmp) land in Plan 04.
//!
//! Authoritative source: folke/tokyonight.nvim + catppuccin/nvim per-plugin
//! files. See 17-RESEARCH.md §Pattern 4.1 for the full list.

use crate::cli::picker::preview_panel::SemanticColor;

/// Visual style modifiers exposed in nvim's `nvim_set_hl` API. Combined with
/// fg/bg/link in [`HighlightSpec`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Style {
    None,
    Bold,
    Italic,
    Underline,
    Undercurl,
    Reverse,
}

/// One entry in the nvim highlight table. Plan 02's renderer translates each
/// `(name, spec)` pair into `vim.api.nvim_set_hl(0, "<name>", { … })`. When
/// `link` is `Some`, fg/bg/style are ignored and the renderer emits
/// `{ link = "<target>" }` instead.
#[derive(Debug, Clone, Copy)]
pub struct HighlightSpec {
    pub fg: Option<SemanticColor>,
    pub bg: Option<SemanticColor>,
    pub style: Style,
    /// If set, emit `{ link = "<target>" }` instead of an fg/bg/style spec.
    pub link: Option<&'static str>,
}

impl HighlightSpec {
    pub const fn fg(color: SemanticColor) -> Self {
        Self {
            fg: Some(color),
            bg: None,
            style: Style::None,
            link: None,
        }
    }
    pub const fn fg_bg(fg: SemanticColor, bg: SemanticColor) -> Self {
        Self {
            fg: Some(fg),
            bg: Some(bg),
            style: Style::None,
            link: None,
        }
    }
    pub const fn bg_only(bg: SemanticColor) -> Self {
        Self {
            fg: None,
            bg: Some(bg),
            style: Style::None,
            link: None,
        }
    }
    pub const fn styled(fg: SemanticColor, style: Style) -> Self {
        Self {
            fg: Some(fg),
            bg: None,
            style,
            link: None,
        }
    }
    pub const fn styled_fg_bg(fg: SemanticColor, bg: SemanticColor, style: Style) -> Self {
        Self {
            fg: Some(fg),
            bg: Some(bg),
            style,
            link: None,
        }
    }
    pub const fn linked(target: &'static str) -> Self {
        Self {
            fg: None,
            bg: None,
            style: Style::None,
            link: Some(target),
        }
    }
    pub const fn style_only(style: Style) -> Self {
        Self {
            fg: None,
            bg: None,
            style,
            link: None,
        }
    }
}

/// Authoritative `(group_name, spec)` table consumed by the nvim adapter.
/// Order is intentional: nvim resolves links lazily, so the link source must
/// resolve to an actual definition emitted earlier in the same colorscheme
/// file (or a built-in nvim group). Section comments mirror the four
/// coverage buckets called out in `17-RESEARCH.md` §Pattern 4.1.
///
/// RED-stage scaffolding: populated in the GREEN step below.
pub static HIGHLIGHT_GROUPS: &[(&str, HighlightSpec)] = &[];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::ThemeRegistry;
    use std::collections::HashSet;

    /// Plan 01 floor: 80 base + 40 diff/LSP-attr + 100 treesitter + 42 LSP = 262.
    /// Plan 04 will add ~130 plugin entries on top to hit the D-06 ~300 target.
    #[test]
    fn group_count_meets_coverage_floor() {
        assert!(
            HIGHLIGHT_GROUPS.len() >= 262,
            "Plan 01 floor: expected ≥ 262 entries, got {}",
            HIGHLIGHT_GROUPS.len()
        );
    }

    /// Every fg/bg `SemanticColor` referenced by an entry must resolve to a
    /// well-formed hex on every embedded theme. This guards both the new
    /// Plan-01 SemanticColor variants and the cascading fallbacks landed in
    /// Task 1 — a missing fallback for Solarized would surface here.
    #[test]
    fn every_entry_resolves_for_every_theme() {
        let registry = ThemeRegistry::new().expect("registry init");
        for (name, spec) in HIGHLIGHT_GROUPS {
            if let Some(fg) = spec.fg {
                for theme in registry.all() {
                    let hex = theme.palette.resolve(fg);
                    assert_eq!(
                        hex.len(),
                        7,
                        "group {} fg on theme {}: bad hex {:?}",
                        name,
                        theme.id,
                        hex
                    );
                }
            }
            if let Some(bg) = spec.bg {
                for theme in registry.all() {
                    let hex = theme.palette.resolve(bg);
                    assert_eq!(
                        hex.len(),
                        7,
                        "group {} bg on theme {}: bad hex {:?}",
                        name,
                        theme.id,
                        hex
                    );
                }
            }
        }
    }

    #[test]
    fn core_base_ui_groups_present() {
        let names: HashSet<&str> = HIGHLIGHT_GROUPS.iter().map(|(n, _)| *n).collect();
        for required in [
            "Normal",
            "NormalNC",
            "NormalFloat",
            "Comment",
            "String",
            "Keyword",
            "Function",
            "Constant",
            "Error",
            "StatusLine",
            "LineNr",
            "DiffAdd",
            "DiffChange",
            "DiffDelete",
            "Pmenu",
            "FloatBorder",
            "Visual",
            "Search",
            "CursorLine",
            "CursorLineNr",
        ] {
            assert!(
                names.contains(required),
                "missing required base-UI group: {}",
                required
            );
        }
    }

    #[test]
    fn core_treesitter_groups_present() {
        let names: HashSet<&str> = HIGHLIGHT_GROUPS.iter().map(|(n, _)| *n).collect();
        for required in ["@comment", "@function", "@keyword", "@string", "@type", "@variable"] {
            assert!(
                names.contains(required),
                "missing required treesitter group: {}",
                required
            );
        }
    }

    #[test]
    fn core_diagnostic_groups_present() {
        let names: HashSet<&str> = HIGHLIGHT_GROUPS.iter().map(|(n, _)| *n).collect();
        for required in [
            "DiagnosticError",
            "DiagnosticWarn",
            "DiagnosticInfo",
            "DiagnosticHint",
        ] {
            assert!(
                names.contains(required),
                "missing required diagnostic group: {}",
                required
            );
        }
    }

    /// Anchor test: the new `LspParameter` SemanticColor variant must be the
    /// fg of `@lsp.type.parameter`. If a future refactor accidentally drops
    /// the variant from this table, plan 02's renderer will lose its only
    /// LSP-parameter color and this test will catch it.
    #[test]
    fn lsp_parameter_group_is_present_and_uses_new_variant() {
        let (_, spec) = HIGHLIGHT_GROUPS
            .iter()
            .find(|(n, _)| *n == "@lsp.type.parameter")
            .expect("@lsp.type.parameter must be in the table");
        assert_eq!(
            spec.fg,
            Some(SemanticColor::LspParameter),
            "@lsp.type.parameter must feed through the new LspParameter variant"
        );
    }

    /// At least 5 entries should use the link form so plan 02 emits compact
    /// `{ link = "..." }` output and stays consistent with tokyonight idiom.
    #[test]
    fn link_style_used_for_at_least_five_entries() {
        let links = HIGHLIGHT_GROUPS.iter().filter(|(_, s)| s.link.is_some()).count();
        assert!(
            links >= 5,
            "expected ≥ 5 link-style entries, found {}",
            links
        );
    }

    /// All highlight group names should be unique — a duplicate would cause
    /// nvim to silently overwrite the earlier entry with the later one and
    /// make the table's emergent ordering meaningful in surprising ways.
    #[test]
    fn group_names_are_unique() {
        let mut seen: HashSet<&str> = HashSet::new();
        for (name, _) in HIGHLIGHT_GROUPS {
            assert!(seen.insert(*name), "duplicate highlight group name: {}", name);
        }
    }

    /// Every link target should either resolve to another entry in the table
    /// or be a well-known nvim built-in name. Detect dangling links early.
    #[test]
    fn link_targets_resolve_or_reference_builtin() {
        let names: HashSet<&str> = HIGHLIGHT_GROUPS.iter().map(|(n, _)| *n).collect();
        // Built-in nvim groups we intentionally link to without redefining.
        let builtin_targets: HashSet<&str> = [
            "Normal",
            "Comment",
            "FloatBorder",
            "Visual",
            "DiffAdd",
            "DiffChange",
            "DiffDelete",
            "Cursor",
            "CursorLine",
        ]
        .into_iter()
        .collect();
        for (name, spec) in HIGHLIGHT_GROUPS {
            if let Some(target) = spec.link {
                assert!(
                    names.contains(target) || builtin_targets.contains(target),
                    "group {} links to unknown target {}",
                    name,
                    target
                );
            }
        }
    }
}
