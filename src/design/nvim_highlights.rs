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
pub static HIGHLIGHT_GROUPS: &[(&str, HighlightSpec)] = &[
    // ── Base UI (80 entries) ──────────────────────────────────────────
    // Source: tokyonight.nvim/lua/tokyonight/groups/base.lua
    (
        "Normal",
        HighlightSpec::fg_bg(SemanticColor::Text, SemanticColor::Background),
    ),
    (
        "NormalNC",
        HighlightSpec::fg_bg(SemanticColor::Text, SemanticColor::Background),
    ),
    (
        "NormalSB",
        HighlightSpec::fg_bg(SemanticColor::Text, SemanticColor::Background),
    ),
    (
        "NormalFloat",
        HighlightSpec::fg_bg(SemanticColor::Text, SemanticColor::Surface),
    ),
    ("FloatBorder", HighlightSpec::fg(SemanticColor::Border)),
    (
        "FloatTitle",
        HighlightSpec::styled(SemanticColor::Accent, Style::Bold),
    ),
    (
        "EndOfBuffer",
        HighlightSpec::fg_bg(SemanticColor::Muted, SemanticColor::Background),
    ),
    (
        "Cursor",
        HighlightSpec::styled_fg_bg(
            SemanticColor::Background,
            SemanticColor::Text,
            Style::Reverse,
        ),
    ),
    ("lCursor", HighlightSpec::linked("Cursor")),
    ("CursorIM", HighlightSpec::linked("Cursor")),
    ("CursorColumn", HighlightSpec::linked("CursorLine")),
    ("CursorLine", HighlightSpec::bg_only(SemanticColor::Surface)),
    (
        "CursorLineNr",
        HighlightSpec::styled(SemanticColor::Accent, Style::Bold),
    ),
    ("LineNr", HighlightSpec::fg(SemanticColor::Muted)),
    ("LineNrAbove", HighlightSpec::fg(SemanticColor::Muted)),
    ("LineNrBelow", HighlightSpec::fg(SemanticColor::Muted)),
    (
        "SignColumn",
        HighlightSpec::fg_bg(SemanticColor::Muted, SemanticColor::Background),
    ),
    (
        "SignColumnSB",
        HighlightSpec::fg_bg(SemanticColor::Muted, SemanticColor::Background),
    ),
    (
        "Folded",
        HighlightSpec::fg_bg(SemanticColor::Muted, SemanticColor::Surface),
    ),
    ("FoldColumn", HighlightSpec::fg(SemanticColor::Muted)),
    (
        "ColorColumn",
        HighlightSpec::bg_only(SemanticColor::Surface),
    ),
    ("Conceal", HighlightSpec::fg(SemanticColor::Muted)),
    ("Directory", HighlightSpec::fg(SemanticColor::FileDir)),
    ("VertSplit", HighlightSpec::fg(SemanticColor::Border)),
    ("WinSeparator", HighlightSpec::fg(SemanticColor::Border)),
    ("DiffAdd", HighlightSpec::bg_only(SemanticColor::GitAdded)),
    (
        "DiffChange",
        HighlightSpec::bg_only(SemanticColor::GitModified),
    ),
    ("DiffDelete", HighlightSpec::bg_only(SemanticColor::Error)),
    (
        "DiffText",
        HighlightSpec::styled(SemanticColor::GitModified, Style::Bold),
    ),
    ("ErrorMsg", HighlightSpec::fg(SemanticColor::Error)),
    ("WarningMsg", HighlightSpec::fg(SemanticColor::Warning)),
    (
        "ModeMsg",
        HighlightSpec::styled(SemanticColor::Accent, Style::Bold),
    ),
    (
        "MoreMsg",
        HighlightSpec::styled(SemanticColor::Accent, Style::Bold),
    ),
    ("MsgArea", HighlightSpec::fg(SemanticColor::Text)),
    ("Question", HighlightSpec::fg(SemanticColor::Accent)),
    (
        "MatchParen",
        HighlightSpec::styled(SemanticColor::Accent, Style::Bold),
    ),
    ("NonText", HighlightSpec::fg(SemanticColor::Muted)),
    ("Whitespace", HighlightSpec::fg(SemanticColor::Muted)),
    (
        "Pmenu",
        HighlightSpec::fg_bg(SemanticColor::Text, SemanticColor::Surface),
    ),
    (
        "PmenuMatch",
        HighlightSpec::styled_fg_bg(SemanticColor::Accent, SemanticColor::Surface, Style::Bold),
    ),
    ("PmenuSel", HighlightSpec::bg_only(SemanticColor::Selection)),
    (
        "PmenuMatchSel",
        HighlightSpec::styled_fg_bg(SemanticColor::Accent, SemanticColor::Selection, Style::Bold),
    ),
    (
        "PmenuSbar",
        HighlightSpec::bg_only(SemanticColor::SurfaceAlt),
    ),
    ("PmenuThumb", HighlightSpec::bg_only(SemanticColor::Border)),
    (
        "StatusLine",
        HighlightSpec::fg_bg(SemanticColor::Text, SemanticColor::SurfaceAlt),
    ),
    (
        "StatusLineNC",
        HighlightSpec::fg_bg(SemanticColor::Muted, SemanticColor::Surface),
    ),
    (
        "TabLine",
        HighlightSpec::fg_bg(SemanticColor::Muted, SemanticColor::Surface),
    ),
    (
        "TabLineFill",
        HighlightSpec::fg_bg(SemanticColor::Muted, SemanticColor::Surface),
    ),
    (
        "TabLineSel",
        HighlightSpec::styled_fg_bg(SemanticColor::Text, SemanticColor::Background, Style::Bold),
    ),
    (
        "Title",
        HighlightSpec::styled(SemanticColor::Accent, Style::Bold),
    ),
    ("Visual", HighlightSpec::bg_only(SemanticColor::Selection)),
    (
        "VisualNOS",
        HighlightSpec::bg_only(SemanticColor::Selection),
    ),
    ("WildMenu", HighlightSpec::linked("Visual")),
    (
        "WinBar",
        HighlightSpec::fg_bg(SemanticColor::Text, SemanticColor::Background),
    ),
    (
        "WinBarNC",
        HighlightSpec::fg_bg(SemanticColor::Muted, SemanticColor::Background),
    ),
    (
        "Search",
        HighlightSpec::styled_fg_bg(
            SemanticColor::Background,
            SemanticColor::Selection,
            Style::Bold,
        ),
    ),
    (
        "IncSearch",
        HighlightSpec::styled_fg_bg(
            SemanticColor::Background,
            SemanticColor::Warning,
            Style::Bold,
        ),
    ),
    (
        "CurSearch",
        HighlightSpec::styled_fg_bg(
            SemanticColor::Background,
            SemanticColor::Warning,
            Style::Bold,
        ),
    ),
    (
        "Substitute",
        HighlightSpec::styled_fg_bg(
            SemanticColor::Background,
            SemanticColor::Selection,
            Style::Bold,
        ),
    ),
    ("SpecialKey", HighlightSpec::fg(SemanticColor::Muted)),
    (
        "SpellBad",
        HighlightSpec::styled(SemanticColor::Error, Style::Undercurl),
    ),
    (
        "SpellCap",
        HighlightSpec::styled(SemanticColor::Warning, Style::Undercurl),
    ),
    (
        "SpellLocal",
        HighlightSpec::styled(SemanticColor::Status, Style::Undercurl),
    ),
    (
        "SpellRare",
        HighlightSpec::styled(SemanticColor::Comment, Style::Undercurl),
    ),
    ("QuickFixLine", HighlightSpec::linked("Visual")),
    ("Bold", HighlightSpec::style_only(Style::Bold)),
    ("Italic", HighlightSpec::style_only(Style::Italic)),
    ("Underlined", HighlightSpec::style_only(Style::Underline)),
    ("Debug", HighlightSpec::fg(SemanticColor::Warning)),
    ("debugBreakpoint", HighlightSpec::fg(SemanticColor::Error)),
    ("debugPC", HighlightSpec::bg_only(SemanticColor::SurfaceAlt)),
    ("Character", HighlightSpec::fg(SemanticColor::String)),
    ("Constant", HighlightSpec::fg(SemanticColor::Number)),
    ("Delimiter", HighlightSpec::fg(SemanticColor::Muted)),
    ("Error", HighlightSpec::fg(SemanticColor::Error)),
    ("Function", HighlightSpec::fg(SemanticColor::Function)),
    ("Identifier", HighlightSpec::fg(SemanticColor::Function)),
    ("Keyword", HighlightSpec::fg(SemanticColor::Keyword)),
    ("Operator", HighlightSpec::fg(SemanticColor::Keyword)),
    ("PreProc", HighlightSpec::fg(SemanticColor::Type)),
    ("Special", HighlightSpec::fg(SemanticColor::Type)),
    ("Statement", HighlightSpec::fg(SemanticColor::Keyword)),
    ("String", HighlightSpec::fg(SemanticColor::String)),
    (
        "Todo",
        HighlightSpec::styled(SemanticColor::Accent, Style::Bold),
    ),
    ("Type", HighlightSpec::fg(SemanticColor::Type)),
    (
        "Comment",
        HighlightSpec::styled(SemanticColor::Comment, Style::Italic),
    ),
    ("MsgSeparator", HighlightSpec::fg(SemanticColor::Border)),
    // ── Diff / diagnostics / LSP attr (40 entries) ────────────────────
    // Source: tokyonight.nvim base.lua diagnostics block
    ("diffAdded", HighlightSpec::fg(SemanticColor::GitAdded)),
    ("diffChanged", HighlightSpec::fg(SemanticColor::GitModified)),
    ("diffRemoved", HighlightSpec::fg(SemanticColor::Error)),
    ("diffFile", HighlightSpec::fg(SemanticColor::Muted)),
    ("diffLine", HighlightSpec::fg(SemanticColor::Muted)),
    ("diffIndexLine", HighlightSpec::fg(SemanticColor::Muted)),
    ("diffOldFile", HighlightSpec::fg(SemanticColor::Muted)),
    ("diffNewFile", HighlightSpec::fg(SemanticColor::Muted)),
    (
        "LspReferenceText",
        HighlightSpec::bg_only(SemanticColor::SurfaceAlt),
    ),
    (
        "LspReferenceRead",
        HighlightSpec::bg_only(SemanticColor::SurfaceAlt),
    ),
    (
        "LspReferenceWrite",
        HighlightSpec::bg_only(SemanticColor::SurfaceAlt),
    ),
    (
        "LspSignatureActiveParameter",
        HighlightSpec::styled(SemanticColor::LspParameter, Style::Bold),
    ),
    (
        "LspCodeLens",
        HighlightSpec::styled(SemanticColor::Muted, Style::Italic),
    ),
    (
        "LspInlayHint",
        HighlightSpec::styled(SemanticColor::Muted, Style::Italic),
    ),
    ("LspInfoBorder", HighlightSpec::linked("FloatBorder")),
    ("DiagnosticError", HighlightSpec::fg(SemanticColor::Error)),
    ("DiagnosticWarn", HighlightSpec::fg(SemanticColor::Warning)),
    ("DiagnosticInfo", HighlightSpec::fg(SemanticColor::Status)),
    ("DiagnosticHint", HighlightSpec::fg(SemanticColor::Comment)),
    (
        "DiagnosticUnnecessary",
        HighlightSpec::styled(SemanticColor::Muted, Style::Italic),
    ),
    (
        "DiagnosticVirtualTextError",
        HighlightSpec::styled(SemanticColor::Error, Style::Italic),
    ),
    (
        "DiagnosticVirtualTextWarn",
        HighlightSpec::styled(SemanticColor::Warning, Style::Italic),
    ),
    (
        "DiagnosticVirtualTextInfo",
        HighlightSpec::styled(SemanticColor::Status, Style::Italic),
    ),
    (
        "DiagnosticVirtualTextHint",
        HighlightSpec::styled(SemanticColor::Comment, Style::Italic),
    ),
    (
        "DiagnosticUnderlineError",
        HighlightSpec::styled(SemanticColor::Error, Style::Undercurl),
    ),
    (
        "DiagnosticUnderlineWarn",
        HighlightSpec::styled(SemanticColor::Warning, Style::Undercurl),
    ),
    (
        "DiagnosticUnderlineInfo",
        HighlightSpec::styled(SemanticColor::Status, Style::Undercurl),
    ),
    (
        "DiagnosticUnderlineHint",
        HighlightSpec::styled(SemanticColor::Comment, Style::Undercurl),
    ),
    ("healthError", HighlightSpec::fg(SemanticColor::Error)),
    ("healthSuccess", HighlightSpec::fg(SemanticColor::GitAdded)),
    ("healthWarning", HighlightSpec::fg(SemanticColor::Warning)),
    ("ComplHint", HighlightSpec::fg(SemanticColor::Comment)),
    ("dosIniLabel", HighlightSpec::fg(SemanticColor::Keyword)),
    ("helpCommand", HighlightSpec::fg(SemanticColor::Accent)),
    ("helpExample", HighlightSpec::fg(SemanticColor::String)),
    (
        "htmlH1",
        HighlightSpec::styled(SemanticColor::Accent, Style::Bold),
    ),
    (
        "htmlH2",
        HighlightSpec::styled(SemanticColor::Keyword, Style::Bold),
    ),
    ("qfFileName", HighlightSpec::fg(SemanticColor::Accent)),
    ("qfLineNr", HighlightSpec::fg(SemanticColor::Muted)),
    ("Question_NC", HighlightSpec::fg(SemanticColor::Muted)),
    // ── Treesitter (~100 entries) ─────────────────────────────────────
    // Source: tokyonight.nvim/lua/tokyonight/groups/treesitter.lua
    ("@annotation", HighlightSpec::fg(SemanticColor::Type)),
    ("@attribute", HighlightSpec::fg(SemanticColor::Type)),
    ("@boolean", HighlightSpec::fg(SemanticColor::Number)),
    ("@character", HighlightSpec::fg(SemanticColor::Number)),
    (
        "@character.printf",
        HighlightSpec::fg(SemanticColor::Number),
    ),
    (
        "@character.special",
        HighlightSpec::fg(SemanticColor::Number),
    ),
    (
        "@comment",
        HighlightSpec::styled(SemanticColor::Comment, Style::Italic),
    ),
    (
        "@comment.error",
        HighlightSpec::styled(SemanticColor::Error, Style::Italic),
    ),
    (
        "@comment.hint",
        HighlightSpec::styled(SemanticColor::Comment, Style::Italic),
    ),
    (
        "@comment.info",
        HighlightSpec::styled(SemanticColor::Status, Style::Italic),
    ),
    (
        "@comment.note",
        HighlightSpec::styled(SemanticColor::Accent, Style::Bold),
    ),
    (
        "@comment.todo",
        HighlightSpec::styled(SemanticColor::Accent, Style::Bold),
    ),
    (
        "@comment.warning",
        HighlightSpec::styled(SemanticColor::Warning, Style::Italic),
    ),
    ("@constant", HighlightSpec::fg(SemanticColor::Number)),
    (
        "@constant.builtin",
        HighlightSpec::fg(SemanticColor::Number),
    ),
    ("@constant.macro", HighlightSpec::fg(SemanticColor::Number)),
    ("@constructor", HighlightSpec::fg(SemanticColor::Type)),
    ("@constructor.tsx", HighlightSpec::fg(SemanticColor::Type)),
    ("@diff.delta", HighlightSpec::linked("DiffChange")),
    ("@diff.minus", HighlightSpec::linked("DiffDelete")),
    ("@diff.plus", HighlightSpec::linked("DiffAdd")),
    ("@function", HighlightSpec::fg(SemanticColor::Function)),
    (
        "@function.builtin",
        HighlightSpec::fg(SemanticColor::Function),
    ),
    ("@function.call", HighlightSpec::fg(SemanticColor::Function)),
    (
        "@function.macro",
        HighlightSpec::fg(SemanticColor::Function),
    ),
    (
        "@function.method",
        HighlightSpec::fg(SemanticColor::Function),
    ),
    (
        "@function.method.call",
        HighlightSpec::fg(SemanticColor::Function),
    ),
    ("@keyword", HighlightSpec::fg(SemanticColor::Keyword)),
    (
        "@keyword.conditional",
        HighlightSpec::fg(SemanticColor::Keyword),
    ),
    (
        "@keyword.coroutine",
        HighlightSpec::fg(SemanticColor::Keyword),
    ),
    ("@keyword.debug", HighlightSpec::fg(SemanticColor::Keyword)),
    (
        "@keyword.directive",
        HighlightSpec::fg(SemanticColor::Keyword),
    ),
    (
        "@keyword.directive.define",
        HighlightSpec::fg(SemanticColor::Keyword),
    ),
    (
        "@keyword.exception",
        HighlightSpec::fg(SemanticColor::Keyword),
    ),
    (
        "@keyword.function",
        HighlightSpec::fg(SemanticColor::Keyword),
    ),
    ("@keyword.import", HighlightSpec::fg(SemanticColor::Keyword)),
    (
        "@keyword.operator",
        HighlightSpec::fg(SemanticColor::Keyword),
    ),
    ("@keyword.repeat", HighlightSpec::fg(SemanticColor::Keyword)),
    ("@keyword.return", HighlightSpec::fg(SemanticColor::Keyword)),
    (
        "@keyword.storage",
        HighlightSpec::fg(SemanticColor::Keyword),
    ),
    ("@label", HighlightSpec::fg(SemanticColor::Keyword)),
    ("@markup", HighlightSpec::fg(SemanticColor::Text)),
    ("@markup.emphasis", HighlightSpec::style_only(Style::Italic)),
    (
        "@markup.environment",
        HighlightSpec::fg(SemanticColor::Type),
    ),
    (
        "@markup.environment.name",
        HighlightSpec::fg(SemanticColor::Type),
    ),
    (
        "@markup.heading",
        HighlightSpec::styled(SemanticColor::Type, Style::Bold),
    ),
    ("@markup.italic", HighlightSpec::style_only(Style::Italic)),
    (
        "@markup.link",
        HighlightSpec::styled(SemanticColor::Accent, Style::Underline),
    ),
    (
        "@markup.link.label",
        HighlightSpec::styled(SemanticColor::Accent, Style::Underline),
    ),
    (
        "@markup.link.url",
        HighlightSpec::styled(SemanticColor::Accent, Style::Underline),
    ),
    ("@markup.list", HighlightSpec::fg(SemanticColor::Warning)),
    (
        "@markup.list.checked",
        HighlightSpec::fg(SemanticColor::Accent),
    ),
    (
        "@markup.list.unchecked",
        HighlightSpec::fg(SemanticColor::Muted),
    ),
    ("@markup.math", HighlightSpec::fg(SemanticColor::String)),
    ("@markup.raw", HighlightSpec::fg(SemanticColor::String)),
    (
        "@markup.raw.markdown_inline",
        HighlightSpec::fg(SemanticColor::String),
    ),
    ("@markup.strikethrough", HighlightSpec::linked("Comment")),
    ("@markup.strong", HighlightSpec::style_only(Style::Bold)),
    (
        "@markup.underline",
        HighlightSpec::style_only(Style::Underline),
    ),
    ("@module", HighlightSpec::fg(SemanticColor::Type)),
    ("@module.builtin", HighlightSpec::fg(SemanticColor::Type)),
    ("@namespace.builtin", HighlightSpec::fg(SemanticColor::Type)),
    ("@none", HighlightSpec::linked("Normal")),
    ("@number", HighlightSpec::fg(SemanticColor::Number)),
    ("@number.float", HighlightSpec::fg(SemanticColor::Number)),
    ("@operator", HighlightSpec::fg(SemanticColor::Keyword)),
    ("@property", HighlightSpec::fg(SemanticColor::LspParameter)),
    (
        "@punctuation.bracket",
        HighlightSpec::fg(SemanticColor::Muted),
    ),
    (
        "@punctuation.delimiter",
        HighlightSpec::fg(SemanticColor::Muted),
    ),
    (
        "@punctuation.special",
        HighlightSpec::fg(SemanticColor::Muted),
    ),
    ("@string", HighlightSpec::fg(SemanticColor::String)),
    (
        "@string.documentation",
        HighlightSpec::fg(SemanticColor::String),
    ),
    ("@string.escape", HighlightSpec::fg(SemanticColor::Accent)),
    ("@string.regexp", HighlightSpec::fg(SemanticColor::Accent)),
    ("@tag", HighlightSpec::fg(SemanticColor::Keyword)),
    (
        "@tag.attribute",
        HighlightSpec::fg(SemanticColor::LspParameter),
    ),
    ("@tag.delimiter", HighlightSpec::fg(SemanticColor::Muted)),
    (
        "@tag.delimiter.tsx",
        HighlightSpec::fg(SemanticColor::Muted),
    ),
    ("@tag.tsx", HighlightSpec::fg(SemanticColor::Keyword)),
    ("@tag.javascript", HighlightSpec::fg(SemanticColor::Keyword)),
    ("@type", HighlightSpec::fg(SemanticColor::Type)),
    ("@type.builtin", HighlightSpec::fg(SemanticColor::Type)),
    ("@type.definition", HighlightSpec::fg(SemanticColor::Type)),
    ("@type.qualifier", HighlightSpec::fg(SemanticColor::Type)),
    ("@variable", HighlightSpec::fg(SemanticColor::Text)),
    ("@variable.builtin", HighlightSpec::fg(SemanticColor::Type)),
    (
        "@variable.member",
        HighlightSpec::fg(SemanticColor::LspParameter),
    ),
    (
        "@variable.parameter",
        HighlightSpec::fg(SemanticColor::LspParameter),
    ),
    (
        "@variable.parameter.builtin",
        HighlightSpec::fg(SemanticColor::LspParameter),
    ),
    // Treesitter aliases for older grammars (pre-0.10)
    ("@namespace", HighlightSpec::fg(SemanticColor::Type)),
    ("@field", HighlightSpec::linked("@variable.member")),
    ("@parameter", HighlightSpec::linked("@variable.parameter")),
    ("@text", HighlightSpec::fg(SemanticColor::Text)),
    ("@text.literal", HighlightSpec::fg(SemanticColor::String)),
    ("@text.reference", HighlightSpec::fg(SemanticColor::Accent)),
    (
        "@text.title",
        HighlightSpec::styled(SemanticColor::Type, Style::Bold),
    ),
    (
        "@text.uri",
        HighlightSpec::styled(SemanticColor::Accent, Style::Underline),
    ),
    (
        "@text.todo",
        HighlightSpec::styled(SemanticColor::Accent, Style::Bold),
    ),
    (
        "@text.note",
        HighlightSpec::styled(SemanticColor::Status, Style::Bold),
    ),
    (
        "@text.warning",
        HighlightSpec::styled(SemanticColor::Warning, Style::Bold),
    ),
    (
        "@text.danger",
        HighlightSpec::styled(SemanticColor::Error, Style::Bold),
    ),
    // ── LSP semantic tokens (~42 entries) ─────────────────────────────
    // Source: tokyonight.nvim/lua/tokyonight/groups/semantic_tokens.lua
    ("@lsp.type.boolean", HighlightSpec::linked("@boolean")),
    (
        "@lsp.type.builtinType",
        HighlightSpec::linked("@type.builtin"),
    ),
    ("@lsp.type.comment", HighlightSpec::linked("@comment")),
    ("@lsp.type.decorator", HighlightSpec::linked("@attribute")),
    (
        "@lsp.type.deriveHelper",
        HighlightSpec::linked("@attribute"),
    ),
    ("@lsp.type.enum", HighlightSpec::linked("@type")),
    (
        "@lsp.type.enumMember",
        HighlightSpec::fg(SemanticColor::Number),
    ),
    (
        "@lsp.type.escapeSequence",
        HighlightSpec::linked("@string.escape"),
    ),
    (
        "@lsp.type.formatSpecifier",
        HighlightSpec::fg(SemanticColor::Accent),
    ),
    ("@lsp.type.generic", HighlightSpec::linked("@type")),
    ("@lsp.type.interface", HighlightSpec::linked("@type")),
    ("@lsp.type.keyword", HighlightSpec::linked("@keyword")),
    (
        "@lsp.type.lifetime",
        HighlightSpec::fg(SemanticColor::Keyword),
    ),
    ("@lsp.type.namespace", HighlightSpec::linked("@namespace")),
    (
        "@lsp.type.namespace.python",
        HighlightSpec::linked("@variable"),
    ),
    ("@lsp.type.number", HighlightSpec::linked("@number")),
    ("@lsp.type.operator", HighlightSpec::linked("@operator")),
    (
        "@lsp.type.parameter",
        HighlightSpec::fg(SemanticColor::LspParameter),
    ),
    ("@lsp.type.property", HighlightSpec::linked("@property")),
    (
        "@lsp.type.selfKeyword",
        HighlightSpec::styled(SemanticColor::LspParameter, Style::Italic),
    ),
    (
        "@lsp.type.selfTypeKeyword",
        HighlightSpec::styled(SemanticColor::LspParameter, Style::Italic),
    ),
    ("@lsp.type.string", HighlightSpec::linked("@string")),
    (
        "@lsp.type.typeAlias",
        HighlightSpec::linked("@type.definition"),
    ),
    (
        "@lsp.type.unresolvedReference",
        HighlightSpec::styled(SemanticColor::Error, Style::Undercurl),
    ),
    ("@lsp.type.variable", HighlightSpec::linked("@variable")),
    (
        "@lsp.typemod.class.defaultLibrary",
        HighlightSpec::styled(SemanticColor::Type, Style::Italic),
    ),
    (
        "@lsp.typemod.enum.defaultLibrary",
        HighlightSpec::styled(SemanticColor::Type, Style::Italic),
    ),
    (
        "@lsp.typemod.enumMember.defaultLibrary",
        HighlightSpec::styled(SemanticColor::Number, Style::Italic),
    ),
    (
        "@lsp.typemod.function.defaultLibrary",
        HighlightSpec::styled(SemanticColor::Function, Style::Italic),
    ),
    (
        "@lsp.typemod.keyword.async",
        HighlightSpec::styled(SemanticColor::Keyword, Style::Italic),
    ),
    (
        "@lsp.typemod.keyword.injected",
        HighlightSpec::styled(SemanticColor::Keyword, Style::Italic),
    ),
    (
        "@lsp.typemod.macro.defaultLibrary",
        HighlightSpec::styled(SemanticColor::Function, Style::Italic),
    ),
    (
        "@lsp.typemod.method.defaultLibrary",
        HighlightSpec::styled(SemanticColor::Function, Style::Italic),
    ),
    (
        "@lsp.typemod.operator.injected",
        HighlightSpec::styled(SemanticColor::Keyword, Style::Italic),
    ),
    (
        "@lsp.typemod.string.injected",
        HighlightSpec::styled(SemanticColor::String, Style::Italic),
    ),
    (
        "@lsp.typemod.struct.defaultLibrary",
        HighlightSpec::styled(SemanticColor::Type, Style::Italic),
    ),
    (
        "@lsp.typemod.type.defaultLibrary",
        HighlightSpec::styled(SemanticColor::Type, Style::Italic),
    ),
    (
        "@lsp.typemod.typeAlias.defaultLibrary",
        HighlightSpec::styled(SemanticColor::Type, Style::Italic),
    ),
    (
        "@lsp.typemod.variable.callable",
        HighlightSpec::linked("@function"),
    ),
    (
        "@lsp.typemod.variable.defaultLibrary",
        HighlightSpec::styled(SemanticColor::Type, Style::Italic),
    ),
    (
        "@lsp.typemod.variable.injected",
        HighlightSpec::styled(SemanticColor::Text, Style::Italic),
    ),
    (
        "@lsp.typemod.variable.static",
        HighlightSpec::fg(SemanticColor::Number),
    ),
];

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
        for required in [
            "@comment",
            "@function",
            "@keyword",
            "@string",
            "@type",
            "@variable",
        ] {
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
        let links = HIGHLIGHT_GROUPS
            .iter()
            .filter(|(_, s)| s.link.is_some())
            .count();
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
            assert!(
                seen.insert(*name),
                "duplicate highlight group name: {}",
                name
            );
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
