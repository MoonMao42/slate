//! State machine for 2D picker navigation.
//!
//! Manages vertical (theme) and horizontal (opacity) navigation,
//! snapshots for rollback, and commit tracking.

use crate::error::Result;
use crate::opacity::OpacityPreset;
use crate::theme::{ThemeRegistry, ThemeVariant, FAMILY_SORT_ORDER};

/// State machine for 2D picker navigation.
///
/// Manages:
/// Vertical axis: Theme selection (wraps around registry)
/// Horizontal axis: Opacity selection (hard stop at edges)
/// Original snapshot for rollback on ESC
/// Commit flag for Drop guard behavior
pub struct PickerState {
    /// Theme IDs in family sort order
    theme_ids: Vec<String>,
    /// Current selected theme index (wraps on move_up/move_down)
    selected_theme_index: usize,
    /// Current selected opacity
    selected_opacity: OpacityPreset,
    /// Snapshot of original theme ID for rollback
    original_theme_id: String,
    /// Snapshot of original opacity for rollback
    original_opacity: OpacityPreset,
    /// Has this state been explicitly committed by the user?
    ///
    /// Shared with `RollbackGuard` via `Rc::clone` so both observe the same
    /// cell — `commit()` flips it to `true` before the guard's Drop runs,
    /// short-circuiting the managed/* rollback. Single-threaded only;
    /// picker runs on the main thread.
    committed: std::rc::Rc<std::cell::Cell<bool>>,
    /// Has user explicitly pressed ←→ to override light-theme opacity guard?
    opacity_override_in_session: bool,
    /// Tab-toggled view mode (D-12). `false` = list-dominant (default per
    /// session; not persisted across picker launches); `true` = full-screen
    /// preview with ◆ Heading responsive fold.
    pub preview_mode_full: bool,
    /// Theme-id → forked starship prompt cache. Populated by Plan 19-07
    /// event_loop glue when Tab mode triggers a fork for a new theme;
    /// cleared on resize (because `--terminal-width` changes).
    ///
    /// Plan 19-06 (Wave 3, parallel plan) owns the full field + accessor
    /// surface; this worktree adds the minimum (field + 3 pub(crate)
    /// methods) needed for Plan 19-07 event_loop glue to compile. The
    /// orchestrator's Wave-3 merge will keep 19-06's authoritative copy.
    prompt_cache: std::collections::HashMap<String, String>,
}

impl PickerState {
    /// Create new picker state, loading theme IDs from registry in order.
    ///
    /// Takes current theme_id and opacity as starting point (snapshots for rollback).
    /// Positions cursor on the current theme, and sets current opacity.
    pub fn new(current_theme_id: &str, current_opacity: OpacityPreset) -> Result<Self> {
        let registry = ThemeRegistry::new()?;

        // Build ordered theme ID list from FAMILY_SORT_ORDER
        let mut theme_ids = Vec::new();
        let by_family = registry.by_family();

        for family_name in FAMILY_SORT_ORDER.iter() {
            if let Some(themes_in_family) = by_family.get(*family_name) {
                // Sort themes within family alphabetically by ID for consistency
                let mut family_themes: Vec<_> =
                    themes_in_family.iter().map(|t| t.id.clone()).collect();
                family_themes.sort();
                theme_ids.extend(family_themes);
            }
        }

        // If no themes match FAMILY_SORT_ORDER, fall back to all themes sorted by ID
        if theme_ids.is_empty() {
            let all_themes = registry.all();
            let mut all_ids: Vec<_> = all_themes.iter().map(|t| t.id.clone()).collect();
            all_ids.sort();
            theme_ids = all_ids;
        }

        // Find the index of the current theme
        let selected_theme_index = theme_ids
            .iter()
            .position(|id| id == current_theme_id)
            .unwrap_or(0);

        Ok(Self {
            theme_ids,
            selected_theme_index,
            selected_opacity: current_opacity,
            original_theme_id: current_theme_id.to_string(),
            original_opacity: current_opacity,
            committed: std::rc::Rc::new(std::cell::Cell::new(false)),
            opacity_override_in_session: false,
            preview_mode_full: false, // D-12 default; Tab toggles
            prompt_cache: std::collections::HashMap::new(),
        })
    }

    /// Get current theme ID
    pub fn get_current_theme_id(&self) -> &str {
        &self.theme_ids[self.selected_theme_index]
    }

    /// Get current opacity
    pub fn get_current_opacity(&self) -> OpacityPreset {
        self.selected_opacity
    }

    /// Get reference to theme IDs list (for external iteration)
    pub fn theme_ids(&self) -> &[String] {
        &self.theme_ids
    }

    /// Current theme cursor index (for rendering scroll window).
    pub fn selected_theme_index(&self) -> usize {
        self.selected_theme_index
    }

    /// Original theme ID captured at picker launch (for rollback).
    pub fn original_theme_id(&self) -> &str {
        &self.original_theme_id
    }

    /// Original opacity captured at picker launch (for rollback).
    pub fn original_opacity(&self) -> OpacityPreset {
        self.original_opacity
    }

    /// Whether the user pressed Enter to commit this selection.
    pub fn is_committed(&self) -> bool {
        self.committed.get()
    }

    /// Returns a clone of the `Rc<Cell<bool>>` committed flag so the event
    /// loop can hand it to `RollbackGuard::arm` (so both the guard's Drop
    /// impl and the panic hook observe the same commit state).
    ///
    /// Currently only consumed by the `committed_flag_shared_with_guard`
    /// unit test; Plan 19-07 (event_loop wiring) will wire this into
    /// `launch_picker` so the `#[allow(dead_code)]` drops there.
    #[allow(dead_code)]
    pub(super) fn committed_flag(&self) -> std::rc::Rc<std::cell::Cell<bool>> {
        self.committed.clone()
    }

    /// Read cached forked prompt for a theme. `None` = cache miss or
    /// first Tab visit. Plan 19-07 glue consults this before forking.
    #[allow(dead_code)] // Wired by Plan 19-07 event_loop Tab branch.
    pub(crate) fn cached_prompt(&self, theme_id: &str) -> Option<&str> {
        self.prompt_cache.get(theme_id).map(String::as_str)
    }

    /// Store a forked prompt for reuse on subsequent Tab visits to
    /// the same theme. Plan 19-07 calls this after a successful fork.
    #[allow(dead_code)] // Wired by Plan 19-07 event_loop Tab branch.
    pub(crate) fn cache_prompt(&mut self, theme_id: &str, prompt: String) {
        self.prompt_cache.insert(theme_id.to_string(), prompt);
    }

    /// Clear the prompt cache — called on terminal resize because
    /// `--terminal-width` is part of the fork args so cached prompts
    /// no longer match the current layout (D-06).
    #[allow(dead_code)] // Wired by Plan 19-07 event_loop resize branch.
    pub(crate) fn invalidate_prompt_cache(&mut self) {
        self.prompt_cache.clear();
    }

    /// Jump to a specific theme by index (for resume-auto and mouse clicks)
    pub fn jump_to_theme(&mut self, index: usize) {
        if index < self.theme_ids.len() {
            self.selected_theme_index = index;
        }
    }

    /// Move up in theme list (wraps around)
    pub fn move_up(&mut self) {
        if self.selected_theme_index == 0 {
            self.selected_theme_index = self.theme_ids.len() - 1;
        } else {
            self.selected_theme_index -= 1;
        }
    }

    /// Move down in theme list (wraps around)
    pub fn move_down(&mut self) {
        self.selected_theme_index = (self.selected_theme_index + 1) % self.theme_ids.len();
    }

    /// Move left in opacity (hard stop at Solid, no wrap)
    /// Returns true if at edge (for bounce feedback)
    pub fn move_left(&mut self) -> bool {
        if self.selected_opacity == OpacityPreset::Solid {
            return true; // at_edge
        }
        self.selected_opacity = match self.selected_opacity {
            OpacityPreset::Clear => OpacityPreset::Frosted,
            OpacityPreset::Frosted => OpacityPreset::Solid,
            OpacityPreset::Solid => OpacityPreset::Solid, // unreachable, guarded above
        };
        false
    }

    /// Move right in opacity (hard stop at Clear, no wrap)
    /// Returns true if at edge (for bounce feedback)
    pub fn move_right(&mut self) -> bool {
        if self.selected_opacity == OpacityPreset::Clear {
            return true; // at_edge
        }
        self.selected_opacity = match self.selected_opacity {
            OpacityPreset::Solid => OpacityPreset::Frosted,
            OpacityPreset::Frosted => OpacityPreset::Clear,
            OpacityPreset::Clear => OpacityPreset::Clear, // unreachable, guarded above
        };
        false
    }

    /// Check if at left opacity edge (Solid)
    pub fn is_at_left_edge(&self) -> bool {
        self.selected_opacity == OpacityPreset::Solid
    }

    /// Check if at right opacity edge (Clear)
    pub fn is_at_right_edge(&self) -> bool {
        self.selected_opacity == OpacityPreset::Clear
    }

    /// Mark opacity as explicitly overridden by user
    pub fn set_opacity_override(&mut self, overridden: bool) {
        self.opacity_override_in_session = overridden;
    }

    /// Check if opacity was explicitly overridden
    pub fn opacity_overridden(&self) -> bool {
        self.opacity_override_in_session
    }

    /// Mark this selection as committed (will skip rollback in Drop).
    ///
    /// Writes through the shared `Rc<Cell<bool>>` so any cloned handle
    /// (e.g. `RollbackGuard`) sees the flip before its own Drop runs.
    pub fn commit(&mut self) {
        self.committed.set(true);
    }

    /// Restore to original snapshot (for rollback)
    pub fn revert(&mut self) {
        if let Some(pos) = self
            .theme_ids
            .iter()
            .position(|id| id == &self.original_theme_id)
        {
            self.selected_theme_index = pos;
        }
        self.selected_opacity = self.original_opacity;
        self.committed.set(false);
    }

    /// Get current theme variant from registry
    pub fn get_current_theme(&self) -> Result<ThemeVariant> {
        let registry = ThemeRegistry::new()?;
        registry
            .get(self.get_current_theme_id())
            .cloned()
            .ok_or_else(|| {
                crate::error::SlateError::InvalidThemeData(format!(
                    "Theme '{}' not found in registry",
                    self.get_current_theme_id()
                ))
            })
    }
}

impl Drop for PickerState {
    /// In-memory-only rollback: reset cursor to original theme + opacity.
    /// Disk-side rollback (managed/*) is handled by `RollbackGuard` in
    /// `rollback_guard.rs` — parallel Drop structure (Phase 19 D-11).
    fn drop(&mut self) {
        if !self.committed.get() {
            self.revert();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_picker_state_creation() {
        let state = PickerState::new("catppuccin-mocha", OpacityPreset::Solid);
        assert!(state.is_ok());
        let state = state.unwrap();
        assert_eq!(state.get_current_theme_id(), "catppuccin-mocha");
        assert_eq!(state.get_current_opacity(), OpacityPreset::Solid);
    }

    #[test]
    fn test_picker_state_theme_wrap_down() {
        let mut state = PickerState::new("catppuccin-mocha", OpacityPreset::Solid).unwrap();

        // Move down to the last theme
        while state.selected_theme_index < state.theme_ids.len() - 1 {
            state.move_down();
        }

        // Move down one more (should wrap to beginning)
        state.move_down();
        assert_eq!(state.selected_theme_index, 0, "Should wrap to first theme");
        assert_eq!(state.get_current_theme_id(), state.theme_ids[0]);
    }

    #[test]
    fn test_picker_state_theme_wrap_up() {
        let mut state = PickerState::new("catppuccin-mocha", OpacityPreset::Solid).unwrap();

        // Move to first theme
        state.selected_theme_index = 0;

        // Move up from first (should wrap to last)
        state.move_up();
        assert_eq!(
            state.selected_theme_index,
            state.theme_ids.len() - 1,
            "Should wrap to last theme"
        );
    }

    #[test]
    fn test_picker_state_opacity_left_hard_stop() {
        let mut state = PickerState::new("catppuccin-mocha", OpacityPreset::Solid).unwrap();

        // Already at Solid; move left should return true (at_edge) and stay at Solid
        let at_edge = state.move_left();
        assert!(at_edge);
        assert_eq!(state.get_current_opacity(), OpacityPreset::Solid);
    }

    #[test]
    fn test_picker_state_opacity_left_transition() {
        let mut state = PickerState::new("catppuccin-mocha", OpacityPreset::Frosted).unwrap();

        // From Frosted, move left should go to Solid, return false (not at edge yet)
        let at_edge = state.move_left();
        assert!(!at_edge);
        assert_eq!(state.get_current_opacity(), OpacityPreset::Solid);
    }

    #[test]
    fn test_picker_state_opacity_right_hard_stop() {
        let mut state = PickerState::new("catppuccin-mocha", OpacityPreset::Clear).unwrap();

        // Already at Clear; move right should return true (at_edge) and stay at Clear
        let at_edge = state.move_right();
        assert!(at_edge);
        assert_eq!(state.get_current_opacity(), OpacityPreset::Clear);
    }

    #[test]
    fn test_picker_state_opacity_right_transition() {
        let mut state = PickerState::new("catppuccin-mocha", OpacityPreset::Frosted).unwrap();

        // From Frosted, move right should go to Clear, return false (not at edge yet)
        let at_edge = state.move_right();
        assert!(!at_edge);
        assert_eq!(state.get_current_opacity(), OpacityPreset::Clear);
    }

    #[test]
    fn test_picker_state_opacity_both_directions() {
        let mut state = PickerState::new("catppuccin-mocha", OpacityPreset::Frosted).unwrap();

        // Frosted → right → Clear
        state.move_right();
        assert_eq!(state.get_current_opacity(), OpacityPreset::Clear);

        // Clear → left → Frosted
        state.move_left();
        assert_eq!(state.get_current_opacity(), OpacityPreset::Frosted);

        // Frosted → left → Solid
        state.move_left();
        assert_eq!(state.get_current_opacity(), OpacityPreset::Solid);
    }

    #[test]
    fn test_picker_state_edge_detection() {
        let mut state = PickerState::new("catppuccin-mocha", OpacityPreset::Solid).unwrap();

        assert!(state.is_at_left_edge());
        assert!(!state.is_at_right_edge());

        state.move_right();
        assert!(!state.is_at_left_edge());
        assert!(!state.is_at_right_edge());

        state.move_right();
        assert!(!state.is_at_left_edge());
        assert!(state.is_at_right_edge());
    }

    #[test]
    fn test_picker_state_opacity_override() {
        let mut state = PickerState::new("catppuccin-mocha", OpacityPreset::Solid).unwrap();

        assert!(!state.opacity_overridden());
        state.set_opacity_override(true);
        assert!(state.opacity_overridden());
        state.set_opacity_override(false);
        assert!(!state.opacity_overridden());
    }

    #[test]
    fn test_picker_state_commit_flag() {
        let mut state = PickerState::new("catppuccin-mocha", OpacityPreset::Solid).unwrap();

        assert!(!state.committed.get());
        state.commit();
        assert!(state.committed.get());
    }

    #[test]
    fn test_picker_state_revert() {
        let mut state = PickerState::new("catppuccin-mocha", OpacityPreset::Solid).unwrap();

        // Change theme and opacity
        state.move_down();
        state.move_right();

        // Verify changes
        assert_ne!(state.get_current_theme_id(), "catppuccin-mocha");
        assert_eq!(state.get_current_opacity(), OpacityPreset::Frosted);

        // Revert
        state.revert();
        assert_eq!(state.get_current_theme_id(), "catppuccin-mocha");
        assert_eq!(state.get_current_opacity(), OpacityPreset::Solid);
    }

    #[test]
    fn test_picker_state_jump_to_theme() {
        let mut state = PickerState::new("catppuccin-mocha", OpacityPreset::Solid).unwrap();
        let theme_count = state.theme_ids.len();

        if theme_count > 1 {
            let target_idx = theme_count - 1;
            state.jump_to_theme(target_idx);
            assert_eq!(state.selected_theme_index, target_idx);
            assert_eq!(state.get_current_theme_id(), state.theme_ids[target_idx]);
        }
    }

    #[test]
    fn test_picker_state_family_sort_order() {
        let state = PickerState::new("catppuccin-mocha", OpacityPreset::Solid).unwrap();

        // Theme list should start with Catppuccin themes
        assert!(!state.theme_ids.is_empty());

        // Find indices of families to verify order
        let first_catppuccin = state
            .theme_ids
            .iter()
            .position(|id| id.starts_with("catppuccin"));
        let first_tokyo = state
            .theme_ids
            .iter()
            .position(|id| id.starts_with("tokyo"));

        if let (Some(cat_idx), Some(tokyo_idx)) = (first_catppuccin, first_tokyo) {
            // Catppuccin should come before Tokyo Night
            assert!(
                cat_idx < tokyo_idx,
                "Catppuccin should come before Tokyo Night per FAMILY_SORT_ORDER"
            );
        }
    }

    #[test]
    fn test_picker_state_get_current_theme() {
        let state = PickerState::new("catppuccin-mocha", OpacityPreset::Solid).unwrap();
        let theme = state.get_current_theme();
        assert!(theme.is_ok());
        let theme = theme.unwrap();
        assert_eq!(theme.id, "catppuccin-mocha");
    }

    // ---------------------------------------------------------------------
    // Phase 19 Plan 19-03 — Task 01 additions (D-08 + D-12 invariants)
    // ---------------------------------------------------------------------

    /// D-12 default: picker opens in list-dominant mode, NOT full preview.
    #[test]
    fn preview_mode_full_defaults_to_list_dominant() {
        let state = PickerState::new("catppuccin-mocha", OpacityPreset::Solid).unwrap();
        assert!(
            !state.preview_mode_full,
            "picker must open in list-dominant mode per D-12"
        );
    }

    /// D-08: family section headers are render-time decoration, not data.
    /// `theme_ids` must contain only real variant IDs (none with the
    /// lavender ◆ prefix or a bare family name like "Catppuccin").
    #[test]
    fn family_headers_are_not_in_theme_ids() {
        let state = PickerState::new("catppuccin-mocha", OpacityPreset::Solid).unwrap();
        for id in state.theme_ids() {
            assert!(
                !id.starts_with("◆"),
                "theme_ids must not carry the lavender ◆ family header prefix; found: {id}"
            );
            // Family names are capitalised ("Catppuccin"); variant IDs are
            // kebab-case ("catppuccin-mocha"). A raw family name in the
            // vector would mean the render-time band leaked into data.
            for family in FAMILY_SORT_ORDER.iter() {
                assert_ne!(
                    id, family,
                    "bare family name {family:?} must never appear as a theme id"
                );
            }
        }
    }

    /// D-08: cursor moves only over selectable variant IDs. Walking the
    /// full move_down cycle must land on strings the registry resolves.
    #[test]
    fn section_header_not_selectable() {
        let registry = ThemeRegistry::new().expect("registry must build");
        let mut state = PickerState::new("catppuccin-mocha", OpacityPreset::Solid).unwrap();
        let total = state.theme_ids().len();
        assert!(total > 0, "registry must be non-empty for this invariant");

        // Walk the full cycle + one extra to confirm wrap lands on data.
        for _ in 0..=total {
            let id = state.get_current_theme_id().to_string();
            assert!(
                registry.get(&id).is_some(),
                "cursor visited a non-resolving id {id:?}; section headers leaked into theme_ids"
            );
            state.move_down();
        }
    }

    /// Task 19-03-01 Test 4 — the committed flag is shared via Rc<Cell<bool>>
    /// so `RollbackGuard` (Task 19-03-02) can observe commit decisions made
    /// on the state after it clones the cell.
    #[test]
    fn committed_flag_shared_with_guard() {
        let mut state = PickerState::new("catppuccin-mocha", OpacityPreset::Solid).unwrap();
        let guard_view = state.committed_flag();
        assert!(!guard_view.get(), "initial commit flag must be false");
        assert!(
            !state.is_committed(),
            "is_committed must mirror the cell's initial value"
        );

        state.commit();

        assert!(
            guard_view.get(),
            "the guard-held Rc<Cell<bool>> must observe the commit flip"
        );
        assert!(
            state.is_committed(),
            "is_committed must mirror the flipped cell"
        );
    }
}
