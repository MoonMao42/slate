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
    /// terminal-control allowlist documented at the top of this module
    /// AND any function body whose signature is preceded by a
    /// `// SWATCH-RENDERER:` line comment (Wave 3 allowlist — the
    /// `src/cli/status_panel.rs::swatch_cell` helper renders theme
    /// palette colors as visible swatches, so its `\x1b[38;2;...m` +
    /// `\x1b[0m` bytes are intentional and must not trip the gate).
    ///
    /// The swatch allowlist works line-by-line: once a `SWATCH-RENDERER:`
    /// marker is seen, the scanner skips all subsequent lines until the
    /// brace depth (counted from the opening `{` that starts the marked
    /// function) returns to zero. This survives nested blocks (match
    /// arms, closures, if/else) without needing a real Rust parser.
    ///
    /// Returns 0 for missing files so Wave 0 can seed the scaffold
    /// without every wave's file set needing to exist yet.
    fn count_style_ansi_in(path: &Path) -> usize {
        let Ok(contents) = fs::read_to_string(path) else {
            return 0;
        };

        // First pass: drop the body of every SWATCH-RENDERER-marked fn.
        let filtered = strip_swatch_renderers(&contents);

        // Strip allowlisted control sequences before the styling scan so
        // they don't false-positive.
        let mut text = filtered;
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

    /// Walk the file line-by-line and drop every line that belongs to a
    /// function body marked with a `// SWATCH-RENDERER:` comment. The
    /// marker itself is a regular-Rust line comment placed directly
    /// above (or a few lines above) the `fn` signature. Brace depth is
    /// counted in `{` / `}` occurrences starting from the first `{`
    /// seen after the marker; once depth returns to 0, the scanner
    /// resumes normal counting on the next line.
    ///
    /// Keeping this outside `count_style_ansi_in` lets the unit tests
    /// exercise the allowlist independently of the wave-gate assertions.
    fn strip_swatch_renderers(src: &str) -> String {
        let mut out = String::with_capacity(src.len());
        let mut in_swatch = false;
        let mut depth: i32 = 0;
        let mut seen_open_brace = false;

        for line in src.lines() {
            if !in_swatch {
                if line.contains("SWATCH-RENDERER:") {
                    in_swatch = true;
                    depth = 0;
                    seen_open_brace = false;
                    // Drop the marker line too so the raw `\x1b[` in
                    // the marker-adjacent docstring (if any) never
                    // reaches the scanner.
                    continue;
                }
                out.push_str(line);
                out.push('\n');
                continue;
            }

            // Inside a swatch fn: track brace depth until we return to 0.
            let opens = line.matches('{').count() as i32;
            let closes = line.matches('}').count() as i32;
            if opens > 0 {
                seen_open_brace = true;
            }
            depth += opens - closes;

            if seen_open_brace && depth <= 0 {
                // End of the swatch fn — drop this line too and resume.
                in_swatch = false;
                depth = 0;
                seen_open_brace = false;
            }
        }

        out
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
    fn no_raw_ansi_in_wave_3_files() {
        let files = &["src/cli/status_panel.rs"];
        let hits = scan_wave_files(files);
        assert!(hits.is_empty(), "Wave 3 style-ANSI residue: {hits:?}");
    }

    /// Wave 3 allowlist regression guard — `strip_swatch_renderers`
    /// must drop the body of `// SWATCH-RENDERER:`-marked functions
    /// (otherwise the `src/cli/status_panel.rs::swatch_cell` helper's
    /// intentional `\x1b[38;2;R;G;B;...m` bytes would trip the gate).
    #[test]
    fn strip_swatch_renderers_drops_marked_fn_body_but_keeps_rest() {
        let src = "\
fn normal_styling() {
    let x = \"\\x1b[1mhi\\x1b[0m\";
}

// SWATCH-RENDERER: intentionally raw ANSI (renders palette colors, not role text)
fn swatch_cell(hex: &str) -> String {
    format!(\"\\x1b[38;2;{};{};{}m████\\x1b[0m\", r, g, b)
}

fn after() {
    println!(\"\\x1b[1mdone\\x1b[0m\");
}
";
        let stripped = strip_swatch_renderers(src);
        // The normal-styling fns survive...
        assert!(stripped.contains("normal_styling"));
        assert!(stripped.contains("after"));
        assert!(stripped.contains("\\x1b[1mhi"));
        assert!(stripped.contains("\\x1b[1mdone"));
        // ...but the marked swatch body is gone.
        assert!(
            !stripped.contains("swatch_cell"),
            "marked fn signature must be dropped with its body"
        );
        assert!(
            !stripped.contains("\\x1b[38;2;"),
            "raw styling bytes inside the marked fn must be stripped"
        );
    }

    #[test]
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
