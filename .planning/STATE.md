---
gsd_state_version: 1.0
milestone: v2.2
milestone_name: Editor Ecosystem + Polish
current_phase: 16
status: planning
last_updated: "2026-04-18T03:06:56.499Z"
last_activity: 2026-04-18
progress:
  total_phases: 4
  completed_phases: 1
  total_plans: 6
  completed_plans: 6
  percent: 100
---

# State: slate v2.2

**Initialized:** 2026-04-18
**Current Phase:** 16
**Status:** Ready to plan

---

## Current Position

Phase: 15 (palette-showcase-slate-demo) — EXECUTING
Plan: Not started
Status: Executing Phase 15
Last activity: 2026-04-18

**What's Next:**

1. `/gsd-execute-phase 15` — run Wave 0 (scaffolding) then proceed through Waves 1–4
2. After Phase 15 ships, Phase 16 (LS_COLORS/EZA_COLORS) consumes the same `file_type_colors` module introduced in Plan 15-02
3. Phase 17 (editor-adapter spike) is a research-only phase; output is `.planning/spikes/editor-adapter/SPIKE.md`, not `src/` changes
4. Phase 18 (Solarized) ideally lands last so it benefits from the demo + new-terminal-reminder experience

---

## v2.2 Phase Structure

| Phase | Goal | Requirements | Character |
|-------|------|--------------|-----------|
| 15 | `slate demo` single-screen showcase + contextual hint | DEMO-01, DEMO-02 | Build |
| 16 | LS_COLORS/EZA_COLORS from palette + `RequiresNewShell` reminders | LS-01..03, UX-01..03 | Build |
| 17 | Editor plugin license + portability research, go/no-go for v2.3 | EDITOR-01 | Research spike |
| 18 | Solarized dark+light full backend coverage + family grouping in list/picker | FAM-01, FAM-02 | Build |

Coverage: 11/11 requirements mapped, no requirement in multiple phases, no empty phase.

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
Phase 15 (Demo)        ← depends on palette infra (shipped); no v2.2 dep
Phase 16 (CLI Colors + UX)  ← depends on Phase 11 shared-shell (v2.1, shipped)
Phase 17 (Research Spike)   ← no dependencies (pure research artifact)
Phase 18 (Solarized)        ← ideally lands last; amplified by Phase 15 + 16
```

**Parallelism opportunity:** Phases 15, 16, and 17 have no v2.2 internal dependencies and could be worked concurrently; Phase 18 prefers to land after 15 and 16 so the Solarized reveal includes demo + new-terminal UX.

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

- Vim/Neovim production adapter → v2.3 (blocked on Phase 17 research outcome)
- VSCode adapter → deferred indefinitely (JSON merge + profile/workspace fragility)
- `slate export` (palette JSON/env/CSS) → later milestone
- BSD `ls` 8-color fallback → rejected in favor of `coreutils` upgrade path
- Additional theme families beyond Solarized → later milestones (one family per milestone cadence)

---

## Session Continuity

**When Resuming Work:**

1. Check `Current Position` above — v2.2 roadmap is defined; next action is `/gsd-discuss-phase 15`
2. Review `Accumulated Context` (decisions, philosophy, architecture patterns)
3. Open `.planning/ROADMAP.md` for the Phase 15–18 structure and success criteria
4. Open `.planning/REQUIREMENTS.md` for v2.2 requirement IDs and traceability
5. Refer to `.planning/milestones/v2.1-*` for shipped cross-platform core reference

**Key Files:**

- `.planning/PROJECT.md` — Product vision and constraints
- `.planning/REQUIREMENTS.md` — v2.2 requirements with REQ-IDs and traceability
- `.planning/ROADMAP.md` — Full phase structure and success criteria (Phases 10–18)
- `.planning/MILESTONES.md` — Shipped milestone log
- `.planning/milestones/v2.1-ROADMAP.md` — v2.1 archived reference
- `.planning/milestones/v2.0-pre-v2.1/` — Snapshot of the replaced v2.0 requirements and roadmap

**Milestone Notes:**

- v2.2 is deliberately a mix of one research spike (Phase 17) and three build phases (15, 16, 18)
- Phase 17 outputs a spike artifact only — no production code changes
- Phase 16 intentionally bundles LS colors with new-terminal reminders because both touch shell integration surface

---

*Last updated: 2026-04-18 — v2.2 roadmap defined: 4 phases (15–18), 11 requirements, 100% coverage*
