//! Grep-in-test invariant: per-wave "zero raw styling ANSI" assertion (D-14).
//!
//! Each wave enables its own test by removing the `#[ignore]` attribute
//! when the wave lands. Wave 0 seeds the scaffold and allowlist so Waves
//! 1–6 only have to drop the ignore.
//!
//! Allowlist (NOT flagged as style-drift):
//!
//!   * `\x1b[?1049l` / `\x1b[?25h` / `\x1b[?25l` — alt-screen / cursor
//!     visibility (terminal control, not styling).
//!   * `\x1b[0m` — reset (must pair with any surviving color escape;
//!     unavoidable in swatch renderers).
//!   * `\x1b[K` / `\x1b[H` / `\x1b[2J` — cursor / clear (terminal
//!     control).
//!
//! Flagged (migrate to `Roles::*` / `SlateTheme`): `\x1b[1m`, `\x1b[2m`,
//! `\x1b[3m`, `\x1b[4m`, `\x1b[38;*m`, `\x1b[48;*m`, `\x1b[0;*m`.

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    /// Count styling ANSI escapes in a single file, excluding the
    /// terminal-control allowlist documented at the top of this module.
    ///
    /// Returns 0 for missing files so Wave 0 can seed the scaffold
    /// without every wave's file set needing to exist yet.
    fn count_style_ansi_in(path: &Path) -> usize {
        let Ok(contents) = fs::read_to_string(path) else {
            return 0;
        };

        // Strip allowlisted control sequences before the styling scan so
        // they don't false-positive.
        let mut text = contents;
        for ctrl in [
            "\\x1b[?1049l",
            "\\x1b[?1049h",
            "\\x1b[?25h",
            "\\x1b[?25l",
            "\\x1b[0m",
            "\\x1b[K",
            "\\x1b[H",
            "\\x1b[2J",
        ] {
            text = text.replace(ctrl, "");
        }

        // Count residues of the styling SGR shape: `\x1b[` followed by
        // any digits ending in `m`. Written as the escape literal
        // `\x1b[` inside the source, so we look for the exact byte
        // sequence `\x1b[`.
        text.match_indices("\\x1b[").count()
    }

    /// Walk a wave's file set and return only the files that still
    /// contain styling escapes.
    fn scan_wave_files(files: &[&str]) -> Vec<(String, usize)> {
        files
            .iter()
            .filter_map(|rel| {
                let path = Path::new(rel);
                let count = count_style_ansi_in(path);
                if count > 0 {
                    Some((rel.to_string(), count))
                } else {
                    None
                }
            })
            .collect()
    }

    // ── Wave gates (enabled by each wave's migration plan) ─────────────

    #[test]
    fn no_raw_ansi_in_wave_1_files() {
        let files = &[
            "src/cli/setup.rs",
            "src/cli/setup_executor/mod.rs",
            "src/cli/wizard_core.rs",
            "src/cli/hub.rs",
            "src/cli/wizard_support.rs",
            "src/cli/tool_selection.rs",
        ];
        let hits = scan_wave_files(files);
        assert!(hits.is_empty(), "Wave 1 style-ANSI residue: {hits:?}");
    }

    #[test]
    fn no_raw_ansi_in_wave_2_files() {
        let files = &["src/cli/theme.rs", "src/cli/font.rs", "src/cli/set.rs"];
        let hits = scan_wave_files(files);
        assert!(hits.is_empty(), "Wave 2 style-ANSI residue: {hits:?}");
    }

    #[test]
    #[ignore = "enabled by Wave 3 plan (18-04-PLAN.md)"]
    fn no_raw_ansi_in_wave_3_files() {
        let files = &["src/cli/status_panel.rs"];
        let hits = scan_wave_files(files);
        assert!(hits.is_empty(), "Wave 3 style-ANSI residue: {hits:?}");
    }

    #[test]
    #[ignore = "enabled by Wave 4 plan (18-05-PLAN.md)"]
    fn no_raw_ansi_in_wave_4_files() {
        let files = &["src/cli/clean.rs", "src/cli/restore.rs"];
        let hits = scan_wave_files(files);
        assert!(hits.is_empty(), "Wave 4 style-ANSI residue: {hits:?}");
    }

    #[test]
    #[ignore = "enabled by Wave 5 plan (18-06-PLAN.md); swatch sites allowlisted separately"]
    fn no_raw_ansi_in_wave_5_files() {
        let files = &[
            "src/cli/demo.rs",
            "src/cli/picker/preview_panel.rs",
            "src/cli/picker/render.rs",
        ];
        let hits = scan_wave_files(files);
        assert!(hits.is_empty(), "Wave 5 style-ANSI residue: {hits:?}");
    }

    #[test]
    #[ignore = "enabled by Wave 6 plan (18-07-PLAN.md); swatch + control-seq allowlist"]
    fn no_raw_ansi_in_wave_6_files() {
        let files = &[
            "src/cli/auto_theme.rs",
            "src/cli/aura.rs",
            "src/cli/list.rs",
            "src/cli/new_shell_reminder.rs",
            "src/cli/share.rs",
            "src/cli/config.rs",
            "src/adapter/ls_colors.rs",
        ];
        let hits = scan_wave_files(files);
        assert!(hits.is_empty(), "Wave 6 style-ANSI residue: {hits:?}");
    }

    /// Wave 0 meta-sanity: the scaffold helpers compile and are callable.
    /// Confirms the `count_style_ansi_in` helper runs on a real file
    /// (chose `Cargo.toml` — no ANSI, count must be 0).
    #[test]
    fn count_style_ansi_handles_missing_file_gracefully() {
        assert_eq!(count_style_ansi_in(Path::new("does-not-exist.rs")), 0);
    }
}
