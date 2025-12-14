//! State machine for 2D picker navigation.
//! Manages vertical (theme) and horizontal (opacity) navigation,
//! snapshots for rollback, and commit tracking.

use crate::error::Result;
use crate::opacity::OpacityPreset;
use crate::theme::{ThemeRegistry, ThemeVariant, FAMILY_SORT_ORDER};

/// State machine for 2D picker navigation.
/// Manages:
/// - Vertical axis: Theme selection (wraps around registry)
/// - Horizontal axis: Opacity selection (hard stop at edges)
/// - Original snapshot for rollback on ESC
/// - Commit flag for Drop guard behavior
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
    committed: bool,
    /// Has user explicitly pressed ←→ to override light-theme opacity guard?
    opacity_override_in_session: bool,
}

impl PickerState {
    /// Create new picker state, loading theme IDs from registry in order.
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
            committed: false,
            opacity_override_in_session: false,
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
        self.committed
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

    /// Mark this selection as committed (will skip rollback in Drop)
    pub fn commit(&mut self) {
        self.committed = true;
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
        self.committed = false;
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
    /// Guard: if not committed, restore original state (rollback safety)
    fn drop(&mut self) {
        if !self.committed {
            self.revert();
            // TODO: Call rollback helper to restore preview path
            // This will be integrated in Task 2
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
        let first_index = state.selected_theme_index;
        let first_theme = state.get_current_theme_id().to_string();

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

        assert!(!state.committed);
        state.commit();
        assert!(state.committed);
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
        // (assuming registry is populated per 06-02)
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
            // Catppuccin should come before Tokyo Night (per)
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
}
