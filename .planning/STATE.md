---
gsd_state_version: 1.0
milestone: v2.2
milestone_name: Editor Ecosystem + Polish
current_phase: 18
status: idle
last_updated: "2026-04-19T15:00:00Z"
last_activity: 2026-04-19 -- Phase 17 closed; v2.2 expanded from 4 to 8 phases (added brand/demo/sound/docs, moved Solarized to Phase 21)
progress:
  total_phases: 8
  completed_phases: 3
  total_plans: 22
  completed_plans: 22
  percent: 37
---

# State: slate v2.2

**Initialized:** 2026-04-18
**Current Phase:** 18 (Brand Sketch + CLI Text-Role System — not yet planned)
**Status:** Phase 17 closed; v2.2 expanded to 8 phases (15-22); awaiting Phase 18 sketch

---

## Current Position

Phase: 18 (brand-sketch-cli-text-roles) — NOT YET PLANNED (sketch phase — starts with `/gsd-sketch`)
Plan: n/a (Phase 18 still TBD)
Plans: 9 of 9 plans complete in Phase 17 (editor-adapter-vim-neovim-colorschemes)
Status: Phase 17 closed 2026-04-19; v2.2 restructured 2026-04-19 to pull v2.3 polish into v2.2; awaiting `/gsd-sketch` for brand text-role system
Last activity: 2026-04-19 -- Phase 17 closed + v2.2 restructured (15-22, 8 phases, Solarized moved to Phase 21)

**What's Next:**

1. `/gsd-sketch` — rapidly mock 3-4 candidate CLI text-role treatments (command keys, paths, shortcuts, status severity) before formalizing Phase 18
2. `/gsd-discuss-phase 18` — refine BRAND-01 / BRAND-02 acceptance criteria using the sketch artifact as the picked direction
3. Remaining v2.2 phases: 18 brand → 19 demo-picker → 20 sound+promo → 21 Solarized → 22 README+release
4. v2.2 ships as one release tag once Phase 22 lands (no per-phase releases)

See also: memory file `project_phase_status.md` for the phase rationale and the polish-seeds that feed phases 18-22 (`project_phase9_promo_plan`, `project_ux_overhaul`, `project_design_*` entries).

---

## v2.2 Phase Structure

| Phase | Goal | Requirements | Character | Status |
|-------|------|--------------|-----------|--------|
| 15 | `slate demo` single-screen showcase + contextual hint | DEMO-01, DEMO-02 | Build | ✅ |
| 16 | LS_COLORS/EZA_COLORS from palette + `RequiresNewShell` reminders | LS-01..03, UX-01..03 | Build | ✅ |
| 17 | Neovim editor adapter (18 Lua colorschemes + loader + hot-reload + 3-way consent) | EDITOR-01 | Build | ✅ |
| 18 | Brand sketch + CLI text-role system | BRAND-01, BRAND-02 | Sketch → Build | 📋 |
| 19 | `slate demo` redesign — picker + live preview across full stack | DEMO-03 | Build | 📋 |
| 20 | Sound design + promo assets (SFX library + VHS recordings) | AUDIO-01 | Build | 📋 |
| 21 | Solarized dark+light + family grouping (moved from Phase 18, lands last for max reveal) | FAM-01, FAM-02 | Build | 📋 |
| 22 | README rewrite + release polish (CHANGELOG, brew tap, v2.2 cut) | DOCS-01, DOCS-02 | Docs | 📋 |

Coverage: 17/17 requirements mapped, no requirement in multiple phases, no empty phase.

---

## Accumulated Context

### Critical Decisions (Locked In)

**Decision 1: Product Pivot**

- **From:** themectl (terminal theme switcher)
- **To:** slate (macOS terminal beautification kit; v2.1 expanded to macOS + Linux)
- **Why:** Market gap exists for one-click terminal setup; theme switching alone is too narrow

**Decision 2: Name — slate**

- **Chosen:** slate (crate: slate-cli, binary: slate, brew: slate)
- **Why:** 5 chars, "clean surface" metaphor, available on crates.io as slate-cli

**Decision 3: New Crate, Not Refactor**

- **Chosen:** New crate `slate-cli`, rewrite with new quality bar
- **Why:** Quality bar fundamentally different (setup wizard, three-tier config, brand language); old themectl code useful as reference only

**Decision 4: Tech Stack**

- **Chosen:** Rust + cliclack (wizard) + indicatif (progress bars) + toml_edit/serde_yaml
- **Why:** Single binary, fast `set` (~5ms), cliclack = charm.sh-level beauty for wizard

**Decision 5: Three-Tier Config Architecture**

- **Chosen:** managed/ (ours) → integration (user entry point) → user custom (never touched)
- **Why:** NvChad/LazyVim philosophy — composition over override, never lose user customizations

**Decision 6: Distribution**

- **Chosen:** brew tap (primary) + cargo-dist automation
- **Why:** No star requirement for tap; single binary; user just `brew install`

**Decision 7: Tool Coverage (v2.0)**

- **Core setup:** Ghostty, Alacritty, Starship, bat, delta, eza, lazygit, fastfetch, Nerd Font, zsh-syntax-highlighting
- **Sync if installed:** tmux
- **Deferred:** iTerm2 (complex plist), Warp (proprietary), fish shell, Terminal.app, Kitty (adapter ready, needs implementation)
- **Why:** Focus on achievable, high-impact tools first

**Decision 8: Setup Interaction Model**

- **Wizard mode:** `slate setup` — step-by-step guided flow with cliclack
- **Quick mode:** `slate setup --quick` — all defaults, no questions
- **Set command:** `slate set <theme>` shipped as the initial lightweight switch path
- **Noun-driven aliases:** Phase 7 landed `slate theme`, `slate font`, `slate config` with backward compatibility
- **Idempotent:** Re-running setup adjusts state (add/remove components)
- **Transparent:** Action list shown before execution
- **Why:** 30-second Time-to-Dopamine requires streamlined, forgiving UX

**Decision 9: Brand Language**

- **Tone:** Playful, premium, never generic
- **Examples:** "Brewing your prompt..." not "Installing starship..."
- **Errors:** Beautifully formatted with colors, never raw stack traces
- **Philosophy:** "We sell taste, not code"
- **Why:** Premium feel is the competitive advantage; every output is a moment to delight

**Decision 10: No Accessibility Permissions (Phase 7)**

- **Rule:** slate must NEVER trigger macOS Accessibility permission dialogs
- **Implementation:** Ghostty reload uses its own AppleScript API (`tell application "Ghostty"`) which only triggers Automation permission (works without granting). System Events access (which triggers Accessibility) is completely removed.
- **Why:** Permission popups feel like malware to users; trust is paramount

**Decision 11: Terminal Feature Gating (Phase 7)**

- **Rule:** Exclude-only approach — only Apple_Terminal falls back to plain starship
- **Implementation:** `$TERM_PROGRAM != "Apple_Terminal"` instead of allow-listing every terminal
- **Why:** All modern terminals render Nerd Fonts; allow-list was fragile and missed case variants

**Decision 12: v2.2 Scope — Research Before Editor Build**

- **Rule:** Editor adapter (vim/nvim) is a research spike in v2.2, not a production build
- **Why:** License + portability risks around absorbing upstream colorscheme plugins need to be resolved before committing v2.3 to an integration approach; shipping an uncurated editor adapter would damage "sell taste"
- **Output:** `.planning/spikes/editor-adapter/SPIKE.md` with go/no-go for v2.3
- **⚠ SUPERSEDED (2026-04-18):** The research happened in-flight via `/gsd-explore` + `/gsd-discuss-phase` + `/gsd-research-phase` (see `.planning/phases/17-editor-adapter-vim-neovim-colorschemes/17-CONTEXT.md` and `17-RESEARCH.md`). Phase 17 ships a production nvim-only editor adapter in v2.2 — the v2.3 deferral no longer applies. No separate SPIKE.md artifact was produced; the CONTEXT + RESEARCH documents serve as the go-decision record.

**Decision 13: v2.2 Scope — BSD-`ls` Upgrade, Not Fallback**

- **Rule:** On macOS with only BSD `ls`, slate recommends `brew install coreutils` (for GNU `gls`) instead of writing an 8-color `LSCOLORS` fallback
- **Why:** 8-color LSCOLORS cannot render Catppuccin/Solarized-grade palettes faithfully; a lossy fallback quietly undermines the "one palette across the stack" promise. An honest upgrade path preserves the quality bar.

**Decision 14: v2.2 Scope — `slate export` Deferred**

- **Rule:** Palette JSON/env/CSS export deferred out of v2.2
- **Why:** Keep v2.2 tight on editor-adjacent and UX polish; long-tail tool support is a separate milestone concern

### Product Philosophy (North Star)

1. **Time-to-Dopamine ≤ 30s** — From `brew install` to "wow" in half a minute
2. **Sell taste, not code** — Pre-packaged designer-verified setups; users don't think about color theory
3. **Transparent, never sneaky** — Show full action list before executing; never modify without consent
4. **Idempotent always** — Run `slate setup` 10 times, same result as once
5. **Composition over override** — Three-tier config; never destroy user customizations
6. **Premium in every detail** — Error messages are beautiful; brand language throughout
7. **Lower the aesthetic barrier** — Curated options, not configuration; like wallpaper apps
8. **Never install without consent** — Detect, explain why beautiful, ask permission
9. **Never request scary permissions** — No Accessibility, no admin rights; Automation-only where needed

### Architecture Pattern

**Three-Tier Config:**

```
~/.config/slate/managed/     ← slate writes here (safe to overwrite)
~/.config/{tool}/            ← user's entry file (includes managed + custom)
~/.config/slate/user/        ← user's custom overrides (never touched)
```

**Auto-Theme Architecture (Phase 7):**

```
Ghostty shell session → launches Swift binary (dark-mode-notify) if enabled
  → DistributedNotificationCenter listens for AppleInterfaceThemeChangedNotification
  → executes `slate theme --auto --quiet`
  → Ghostty-only (guarded by $TERM_PROGRAM check in env.zsh)
  → `slate config set auto-theme disable` stops the watcher process
  → `slate status` / `slate clean` inspect and manage the watcher directly
```

**Cross-Tool Color Consistency (Phase 7):**

```
Theme palette (Palette struct) → single source of truth
  → Ghostty: direct palette injection (not theme names)
  → Alacritty: TOML color sections via managed import
  → Starship: [palettes.slate] with adaptive powerline_fg
  → bat/delta/eza/lazygit: theme name or palette mapping
```

### v2.2 Phase Dependencies

```
Phase 15 (Demo)                      ← shipped
Phase 16 (CLI Colors + UX)           ← shipped (depends on Phase 11 shared-shell)
Phase 17 (Neovim Adapter)            ← shipped
Phase 18 (Brand Sketch + Text Roles) ← no hard deps; runs first to feed 19/21/22
Phase 19 (Demo Redesign)             ← depends on Phase 18 (brand roles drive picker chrome)
Phase 20 (Sound + Promo)             ← depends on Phase 18 + 19 (picker is a trigger surface)
Phase 21 (Solarized + Family)        ← scheduled LAST so reveal = brand + demo + sound together
Phase 22 (README + Release)          ← depends on 20 (VHS recordings) + 21 (Solarized ships before README mentions it)
```

**Scheduling note:** Strict linear order 18 → 19 → 20 → 21 → 22. Phase 21 (Solarized) is deliberately delayed so its reveal lands with the full polished experience; Phase 22 (README) runs last so docs capture the shipped v2.2 state.

---

## Deferred Items

Items acknowledged and deferred at v2.1 milestone close on 2026-04-18:

| Category | Item | Status | Notes |
|----------|------|--------|-------|
| uat_gap | 06-HUMAN-UAT.md | completed | 0 pending scenarios; artifact flagged but no open work |
| verification_gap | 06-VERIFICATION.md | human_needed | Phase 06 Interactive Experience (v2.0) — predates v2.1 scope |
| verification_gap | 07-VERIFICATION.md | gaps_found | Phase 07 Polish + Gap Fixes (v2.0) — predates v2.1 scope |

All three items are from v2.0 phases (pre-v2.1). Not blocking v2.2 work.

v2.2-specific deferrals (from REQUIREMENTS.md):

- Vim/Neovim production adapter → **shipped as Phase 17 in v2.2 (2026-04-19)**, nvim-only. Classic vim still out of scope.
- VSCode adapter → deferred indefinitely (JSON merge + profile/workspace fragility)
- `slate export` (palette JSON/env/CSS) → later milestone
- BSD `ls` 8-color fallback → rejected in favor of `coreutils` upgrade path
- Additional theme families beyond Solarized → later milestones (one family per milestone cadence)
- Brand polish / demo redesign / sound / README rewrite → **pulled back into v2.2** (2026-04-19) as Phases 18-22, was originally slated for v2.3

---

## Session Continuity

**When Resuming Work:**

1. Check `Current Position` above — Phase 17 closed, next is `/gsd-sketch` for brand text roles, then `/gsd-discuss-phase 18`
2. Review `Accumulated Context` (decisions, philosophy, architecture patterns)
3. Open `.planning/ROADMAP.md` for the Phase 15–22 structure and success criteria
4. Open `.planning/REQUIREMENTS.md` for v2.2 requirement IDs and traceability
5. Refer to `.planning/milestones/v2.1-*` for shipped cross-platform core reference

**Key Files:**

- `.planning/PROJECT.md` — Product vision and constraints
- `.planning/REQUIREMENTS.md` — v2.2 requirements with REQ-IDs and traceability (17 requirements across 8 phases)
- `.planning/ROADMAP.md` — Full phase structure and success criteria (Phases 10–22)
- `.planning/MILESTONES.md` — Shipped milestone log
- `.planning/milestones/v2.1-ROADMAP.md` — v2.1 archived reference
- `.planning/milestones/v2.0-pre-v2.1/` — Snapshot of the replaced v2.0 requirements and roadmap

**Milestone Notes:**

- v2.2 is now 8 phases covering editor ecosystem (shipped 15-17) + brand / demo / sound / Solarized / release polish (18-22, not yet planned). Ships as a single cohesive release, not per-phase tags.
- Phase 17 shipped a production nvim-only editor adapter — supersedes the original research-spike scope from Decision 12.
- Phase 16 bundles LS colors + new-terminal reminders because both touch shell integration surface.
- Phase 21 (Solarized) is deliberately last so its reveal is amplified by the new brand + demo + sound work from phases 18-20.
- Memory seeds feeding phases 18-22: `project_phase9_promo_plan`, `project_phase9_ui_polish`, `project_ux_overhaul`, `project_design_claude_code_preset`, `project_design_manual_override_lock`, `project_design_ideas_opacity_daynight`, `project_phase6_picker_ux_debt`.

---

*Last updated: 2026-04-19 — v2.2 expanded from 4 to 8 phases (15–22), 17 requirements, 100% coverage. Phase 17 closed; Phase 18 brand sketch next.*
