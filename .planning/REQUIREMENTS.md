# Requirements: slate v2.2

**Defined:** 2026-04-18
**Core Value:** 30-second Time-to-Dopamine: from `brew install` to a stunning terminal
**Milestone Goal:** Extend "one palette across the stack" to CLI file tools, editor ecosystem, and interaction polish — researching before building the editor adapter.

## v2.2 Requirements

Each requirement maps to exactly one roadmap phase. Phases continue numbering from v2.1 (Phases 15–18).

### Demo & Showcase

- [ ] **DEMO-01**: User can run `slate demo` and see a curated, single-screen showcase of the active palette — covering at minimum a code snippet with syntax highlighting, a directory tree (file-type colors), git-log excerpt, and a progress bar — so the "wow" moment is discoverable without hunting for it
- [ ] **DEMO-02**: User sees a demo-style hint (not the full demo, but a pointer like "run `slate demo` to see it in action") after `slate theme set <id>` and `slate setup` so the showcase is surfaced at the right moments without being intrusive

### CLI Tool Colors

- [ ] **LS-01**: User can have a slate-managed `LS_COLORS` environment variable rendered from the active palette, covering the common file-type and extension groups (directory, exec, symlink, archive, media, code, docs, etc.), written to the managed shell integration so every new shell picks it up
- [ ] **LS-02**: User can have a slate-managed `EZA_COLORS` environment variable rendered from the active palette, using eza's own palette keys where they diverge from GNU ls, written alongside `LS_COLORS`
- [ ] **LS-03**: On macOS where BSD `ls` is the default, user sees a one-time capability message recommending `brew install coreutils` (for GNU `gls`) instead of slate silently writing an 8-color `LSCOLORS` fallback

### UX: New-Terminal Reminders

- [ ] **UX-01**: Adapters can declare a `RequiresNewShell` signal on their return value, indicating whether the change they just applied needs a fresh shell session to take effect (env/PATH/integration changes do; file-only writes to already-reloadable tools do not)
- [ ] **UX-02**: At the end of `slate setup`, `slate theme …`, `slate font …`, and `slate config …` runs, slate emits at most one "new-terminal" reminder per run, deduplicated across adapters, only when at least one `RequiresNewShell` signal fired
- [ ] **UX-03**: The reminder text is platform-aware and uses active-experience language (no "please", no "you need to"): on macOS it points to `⌘N`; on Linux it says "open a new terminal"; the phrasing frames the new shell as the reveal of the change, not as a limitation

### Editor Adapter Research

- [ ] **EDITOR-01**: slate has a research artifact (spike-quality, not production code) evaluating whether existing vim/nvim colorscheme plugins — at minimum catppuccin-nvim, tokyonight.nvim, gruvbox.nvim, rose-pine/neovim, solarized — can be absorbed into slate's codebase under a compatible license, and producing a concrete go/no-go recommendation plus an architecture sketch for the v2.3 editor adapter

### Theme Family

- [ ] **FAM-01**: User can apply Solarized Dark and Solarized Light through `slate theme set solarized-dark` / `solarized-light`, with full coverage across existing tool backends (Ghostty, Alacritty, Starship, bat, delta, eza, lazygit, fastfetch, tmux, zsh-syntax-highlighting) matching the quality bar of existing themes
- [ ] **FAM-02**: User can see themes grouped by `family` when listing (`slate theme --list` or equivalent) and when browsing the picker, so the 20 variants across 10 families are navigable rather than a flat list

## Future Requirements (Deferred)

### v2.3 candidates (depend on v2.2 research)

- **EDITOR-02**: Vim/Neovim editor adapter — scope determined by EDITOR-01 outcome (absorb upstream code vs. slate-generated drop-in vs. plugin-manager spec)
- **EDITOR-03**: VSCode adapter — deferred due to settings.json merge + profile/workspace layering fragility

### Later

- **EXPORT-01**: `slate export` — palette exported as JSON / env vars / CSS variables for long-tail tools (fzf, yazi, zellij) — deliberately deferred from v2.2 per user decision

## Out of Scope

| Feature | Reason |
|---------|--------|
| VSCode adapter | JSON merge + profile/workspace layering is too fragile for a drop-in approach; revisit after editor research |
| Vim/Neovim production adapter in v2.2 | Blocked on EDITOR-01 research outcome; shipping without research risks a粗糙 result that damages "sell taste" |
| `slate export` (palette export to JSON/env/CSS) | User chose to defer; keep v2.2 scope tight on editor-adjacent + UX polish |
| BSD `ls` 8-color fallback | Cannot render Catppuccin/Solarized-grade palettes faithfully; recommending `coreutils` is more honest |
| Plugin-manager detection (lazy.nvim / packer / vim-plug) | Rejected by the same principle that rejects VSCode: fragile state detection; conflicts with slate's three-tier config philosophy |
| Shipping additional theme families beyond Solarized in v2.2 | 10 families (Catppuccin, Dracula, Everforest, Gruvbox, Kanagawa, Nord, Rose Pine, Solarized, Tokyo Night) is enough surface area; new families land per-milestone |

## Traceability

| Requirement | Phase | Status |
|-------------|-------|--------|
| DEMO-01 | Phase 15 | Pending |
| DEMO-02 | Phase 15 | Pending |
| LS-01 | Phase 16 | Pending |
| LS-02 | Phase 16 | Pending |
| LS-03 | Phase 16 | Pending |
| UX-01 | Phase 16 | Pending |
| UX-02 | Phase 16 | Pending |
| UX-03 | Phase 16 | Pending |
| EDITOR-01 | Phase 17 | Pending |
| FAM-01 | Phase 18 | Pending |
| FAM-02 | Phase 18 | Pending |

**Coverage:**
- v2.2 requirements: 11 total
- Mapped to phases: 11
- Unmapped: 0 ✓

---

*Requirements defined: 2026-04-18*
*Source: post-v2.1 Gemini strategic review + user decisions during /gsd-new-milestone flow*
