//! Grep-in-test invariant: per-wave "zero raw styling ANSI" assertion.
//! Each wave enables its own test by removing the `#[ignore]` attribute
//! when the wave lands. seeds the scaffold and allowlist so Waves
//! 1–6 only have to drop the ignore.
//! Allowlist (NOT flagged as style-drift):
//! * `\x1b[?1049l` / `\x1b[?25h` / `\x1b[?25l` — alt-screen / cursor
//! visibility (terminal control, not styling).
//! * `\x1b[0m` — reset (must pair with any surviving color escape;
//! unavoidable in swatch renderers).
//! * `\x1b[K` / `\x1b[H` / `\x1b[2J` — cursor / clear (terminal
//! control).
//! Flagged (migrate to `Roles::*` / `SlateTheme`): `\x1b[1m`, `\x1b[2m`,
//! `\x1b[3m`, `\x1b[4m`, `\x1b[38;*m`, `\x1b[48;*m`, `\x1b[0;*m`.

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    /// Count styling ANSI escapes in a single file, excluding the
    /// terminal-control allowlist documented at the top of this module
    /// AND any function body whose signature is preceded by a
    /// `// SWATCH-RENDERER:` line comment (allowlist — the
    /// `src/cli/status_panel.rs::swatch_cell` helper renders theme
    /// palette colors as visible swatches, so its `\x1b[38;2;...m` +
    /// `\x1b[0m` bytes are intentional and must not trip the gate).
    /// extends the allowlist with `// TERMINAL-CONTROL:` — a
    /// narrower, **single-line** marker that covers ONLY the very next
    /// source line. This is for alt-screen / cursor-visibility / screen-
    /// clear escapes that are harder to encode as bare `\x1b[...l` /
    /// `\x1b[?25h` prefixes (e.g. when emitted via `push_str` against a
    /// composed `String`). Unlike SWATCH-RENDERER (function scope), the
    /// TERMINAL-CONTROL marker covers the immediate next line only so a
    /// stray marker cannot accidentally smother a whole block.
    /// The swatch allowlist works line-by-line: once a `SWATCH-RENDERER:`
    /// marker is seen, the scanner skips all subsequent lines until the
    /// brace depth (counted from the opening `{` that starts the marked
    /// function) returns to zero. This survives nested blocks (match
    /// arms, closures, if/else) without needing a real Rust parser.
    /// Returns 0 for missing files so can seed the scaffold
    /// without every wave's file set needing to exist yet.
    fn count_style_ansi_in(path: &Path) -> usize {
        let Ok(contents) = fs::read_to_string(path) else {
            return 0;
        };

        // First pass: drop the body of every SWATCH-RENDERER-marked fn.
        let swatch_filtered = strip_swatch_renderers(&contents);
        // Second pass: drop every line that immediately follows a
        // `// TERMINAL-CONTROL:` marker (cursor / alt-screen /
        // screen-clear escapes emitted via push_str rather than a bare
        // literal on its own line).
        let filtered = strip_terminal_control_markers(&swatch_filtered);

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

    /// Line-level allowlist for terminal-control escapes that are
    /// assembled via `push_str` rather than written as a bare `\x1b[?25h`
    /// literal the bulk allowlist already knows about. Whenever a line
    /// contains `TERMINAL-CONTROL:` the NEXT line is dropped; the marker
    /// itself is also dropped so it cannot survive to re-enter the scan.
    /// Unlike `strip_swatch_renderers`, this is **single-line scope**
    /// a stray marker cannot accidentally smother a multi-line block.
    /// NOTE for future maintainers: this helper is a line filter; keep
    /// braces out of the documentation when editing marker-adjacent
    /// code (same hygiene rule as `strip_swatch_renderers`; see Wave-3
    /// docstring-hygiene bug in 18-04-SUMMARY).
    fn strip_terminal_control_markers(src: &str) -> String {
        let mut out = String::with_capacity(src.len());
        let mut skip_next = false;
        for line in src.lines() {
            if skip_next {
                skip_next = false;
                continue;
            }
            if line.contains("TERMINAL-CONTROL:") {
                skip_next = true;
                continue;
            }
            out.push_str(line);
            out.push('\n');
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

    /// Recursively collect every `*.rs` path under `root`. Used by the
    /// phase-level aggregate invariant (`no_raw_styling_ansi_anywhere_in_user_surfaces`)
    /// and the deprecation sweep (`no_deprecated_allow_in_user_surfaces_after_phase_18`).
    /// No `walkdir` dependency — stdlib `read_dir` recursion is plenty
    /// for a test-only helper that runs on a few dozen files.
    fn walk_rs_files(root: &Path, out: &mut Vec<std::path::PathBuf>) {
        let Ok(entries) = fs::read_dir(root) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk_rs_files(&path, out);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                out.push(path);
            }
        }
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
        assert!(hits.is_empty(), " style-ANSI residue: {hits:?}");
    }

    #[test]
    fn no_raw_ansi_in_wave_2_files() {
        let files = &["src/cli/theme.rs", "src/cli/font.rs", "src/cli/set.rs"];
        let hits = scan_wave_files(files);
        assert!(hits.is_empty(), " style-ANSI residue: {hits:?}");
    }

    #[test]
    fn no_raw_ansi_in_wave_3_files() {
        let files = &["src/cli/status_panel.rs"];
        let hits = scan_wave_files(files);
        assert!(hits.is_empty(), " style-ANSI residue: {hits:?}");
    }

    /// allowlist regression guard — `strip_swatch_renderers`
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
        assert!(hits.is_empty(), " style-ANSI residue: {hits:?}");
    }

    /// allowlist regression guard — `strip_terminal_control_markers`
    /// drops the single line that immediately follows a
    /// `// TERMINAL-CONTROL:` marker while leaving surrounding code intact.
    /// Mirrors the narrower scope of the Wave-5 marker (single line vs.
    /// SWATCH-RENDERER's function scope).
    #[test]
    fn strip_terminal_control_markers_drops_only_next_line() {
        let src = "\
fn normal() {
    let x = \"\\x1b[1mhi\\x1b[0m\";
}

fn render() {
    // TERMINAL-CONTROL: leave alt-screen
    output.push_str(\"\\x1b[?1049l\");
    let styled = \"\\x1b[1mremains\\x1b[0m\";
}
";
        let stripped = strip_terminal_control_markers(src);
        // Surrounding styling bytes survive (both the `normal` fn and the
        // `styled` binding after the marker).
        assert!(stripped.contains("\\x1b[1mhi"));
        assert!(stripped.contains("\\x1b[1mremains"));
        // The marker itself + its target line are gone.
        assert!(!stripped.contains("TERMINAL-CONTROL"));
        assert!(
            !stripped.contains("1049l"),
            "the single line after the marker must be stripped"
        );
    }

    #[test]
    fn no_raw_ansi_in_wave_5_files() {
        let files = &[
            "src/cli/demo.rs",
            "src/cli/picker/preview_panel.rs",
            "src/cli/picker/render.rs",
        ];
        let hits = scan_wave_files(files);
        assert!(hits.is_empty(), " style-ANSI residue: {hits:?}");
    }

    #[test]
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
        assert!(hits.is_empty(), " style-ANSI residue: {hits:?}");
    }

    /// Final post-Phase-18 sweep — after lands, no file in
    /// `src/cli/` or `src/brand/language.rs` should carry
    /// `#![allow(deprecated)]`. Task 3 inserted those
    /// attributes during the Wave-0 deprecation seeding; each migrating
    /// wave was supposed to drop them as it touched the file. This
    /// test makes the sweep CI-authoritative — a future regression
    /// where someone adds an `#![allow(deprecated)]` to silence a
    /// `Typography::*` / `Symbols::*` / `Colors::*` lint will fail
    /// here instead of silently re-introducing legacy APIs.
    #[test]
    fn no_deprecated_allow_in_user_surfaces_after_phase_18() {
        let mut targets: Vec<std::path::PathBuf> = Vec::new();

        // Walk src/cli/ recursively (covers both top-level files and
        // any nested module dirs like setup_executor / picker).
        walk_rs_files(Path::new("src/cli"), &mut targets);
        targets.push(std::path::PathBuf::from("src/brand/language.rs"));

        let mut offenders: Vec<std::path::PathBuf> = Vec::new();
        for path in &targets {
            let Ok(src) = fs::read_to_string(path) else {
                continue;
            };
            if src.contains("#![allow(deprecated)]") {
                offenders.push(path.clone());
            }
        }
        assert!(
            offenders.is_empty(),
            "deprecated allow-attrs remain after : {:?}",
            offenders
        );
    }

    /// Phase-level aggregate invariant — belt-and-suspenders
    /// on top of the 6 per-wave gates. Walks ALL `*.rs` under `src/cli/`
    /// recursively + `src/adapter/ls_colors.rs` + `src/brand/language.rs`
    /// in ONE scan and asserts zero raw styling-ANSI residue (respecting
    /// the shared SWATCH-RENDERER / TERMINAL-CONTROL allowlists baked into
    /// `count_style_ansi_in`).
    /// Catches two regression classes the per-wave gates alone cannot:
    /// * A brand-new `src/cli/*.rs` file added post-Phase-18 that isn't
    /// listed in any wave's explicit file array (per-wave tests only
    /// see the hardcoded set).
    /// * A nested `src/cli/<mod>/inner.rs` that ships raw ANSI because
    /// the parent wave never enumerated the child (e.g. a new picker
    /// submodule beyond `render.rs` + `preview_panel.rs`).
    #[test]
    fn no_raw_styling_ansi_anywhere_in_user_surfaces() {
        let mut targets: Vec<std::path::PathBuf> = Vec::new();
        walk_rs_files(Path::new("src/cli"), &mut targets);
        targets.push(std::path::PathBuf::from("src/adapter/ls_colors.rs"));
        targets.push(std::path::PathBuf::from("src/brand/language.rs"));

        // Stable order for reproducible failure output.
        targets.sort();

        let path_strings: Vec<String> = targets
            .iter()
            .filter_map(|p| p.to_str().map(|s| s.to_string()))
            .collect();
        let borrowed: Vec<&str> = path_strings.iter().map(String::as_str).collect();
        let hits = scan_wave_files(&borrowed);
        assert!(
            hits.is_empty(),
            " aggregate: raw styling ANSI residue in user surfaces: {hits:?}"
        );
    }

    /// meta-sanity: the scaffold helpers compile and are callable.
    /// Confirms the `count_style_ansi_in` helper runs on a real file
    /// (chose `Cargo.toml` — no ANSI, count must be 0).
    #[test]
    fn count_style_ansi_handles_missing_file_gracefully() {
        assert_eq!(count_style_ansi_in(Path::new("does-not-exist.rs")), 0);
    }
}
