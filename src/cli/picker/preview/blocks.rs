//! Pure block renderers for picker preview.
//! SWATCH-RENDERER: intentionally raw ANSI (renders palette colors, not
//! role text). Allowlisted by `brand::migration::tests::no_raw_styling_ansi_anywhere_in_user_surfaces`.
//! Migrated verbatim from `src/cli/demo.rs` in . The 4
//! block renderers (code / tree / git-log / progress) plus the
//! new `render_palette_swatch` are consumed by
//! `src/cli/picker/preview/compose.rs` to assemble the
//! responsive full-preview stack.

use crate::adapter::palette_renderer::PaletteRenderer;
use crate::cli::picker::preview_panel::SemanticColor;
use crate::design::file_type_colors::{classify, FileKind};
use crate::theme::Palette;

// SWATCH-RENDERER: intentionally raw ANSI (renders palette colors, not role text).
// `RESET` pairs with every `fg()` escape below, so it lives inside the same
// allowlist scope.
const RESET: &str = "\x1b[0m";

/// Build an ANSI 24-bit foreground escape from a `#RRGGBB` hex string.
/// Returns an empty string on malformed input — which would be a palette /
/// theme-file bug, not a user-facing error — so the renderer degrades to
/// uncolored text rather than crashing.
// SWATCH-RENDERER: intentionally raw ANSI (renders palette colors, not role text)
fn fg(hex: &str) -> String {
    match PaletteRenderer::hex_to_rgb(hex) {
        Ok((r, g, b)) => format!("\x1b[38;2;{r};{g};{b}m"),
        Err(_) => String::new(),
    }
}

/// Colored span helper: wraps `text` with the foreground-ANSI for `hex` and
/// a trailing RESET so adjacent spans don't bleed.
fn span(out: &mut String, hex: &str, text: &str) {
    out.push_str(&fg(hex));
    out.push_str(text);
    out.push_str(RESET);
}

/// Code block (TypeScript sample). Realises slots 2, 3, 4, 5, 6, 8, 13.
pub fn render_code_block(palette: &Palette) -> String {
    let kw = palette.resolve(SemanticColor::Keyword); // slot 5
    let kw_emph = &palette.bright_magenta; // slot 13 — `type` emphasis
    let ty = palette.resolve(SemanticColor::Type); // slot 6
    let fun = palette.resolve(SemanticColor::Function); // slot 4
    let string = palette.resolve(SemanticColor::String); // slot 2
    let num = palette.resolve(SemanticColor::Number); // slot 3
    let comment = palette.resolve(SemanticColor::Comment); // slot 8

    let mut out = String::with_capacity(512);

    // Line 1: // Fetch a user and greet them.
    span(&mut out, &comment, "// Fetch a user and greet them.");
    out.push('\n');

    // Line 2: type User = { name: string; age: number };
    span(&mut out, kw_emph, "type");
    out.push(' ');
    span(&mut out, &ty, "User");
    out.push_str(" = { name: ");
    span(&mut out, &ty, "string");
    out.push_str("; age: ");
    span(&mut out, &ty, "number");
    out.push_str(" };");
    out.push('\n');

    // Line 3: (blank)
    out.push('\n');

    // Line 4: async function greet(id: string, retries = 42):
    // Promise<void> {
    // Kept as single line per interfaces; total is 58 cols.
    span(&mut out, &kw, "async");
    out.push(' ');
    span(&mut out, &kw, "function");
    out.push(' ');
    span(&mut out, &fun, "greet");
    out.push_str("(id: ");
    span(&mut out, &ty, "string");
    out.push_str(", retries = ");
    span(&mut out, &num, "42");
    out.push_str("): ");
    span(&mut out, &ty, "Promise");
    out.push('<');
    span(&mut out, &ty, "void");
    out.push_str("> {");
    out.push('\n');

    // Line 5: const user: User = await fetchUser(id, retries);
    out.push_str("  ");
    span(&mut out, &kw, "const");
    out.push_str(" user: ");
    span(&mut out, &ty, "User");
    out.push_str(" = ");
    span(&mut out, &kw, "await");
    out.push(' ');
    span(&mut out, &fun, "fetchUser");
    out.push_str("(id, retries);");
    out.push('\n');

    // Line 6: const message = `Hello, ${user.name}! You are ${user.age}.`;
    out.push_str("  ");
    span(&mut out, &kw, "const");
    out.push_str(" message = ");
    span(
        &mut out,
        &string,
        "`Hello, ${user.name}! You are ${user.age}.`",
    );
    out.push(';');
    out.push('\n');

    // Line 7: console.log(message);
    out.push_str("  console.");
    span(&mut out, &fun, "log");
    out.push_str("(message);");
    out.push('\n');

    // Line 8: }
    out.push('}');
    out.push('\n');

    out
}

/// Entry in the demo's static file-tree. Static tuple rather than struct to
/// keep the sample data dense and literal-readable at the call site.
type TreeEntry = (&'static str, FileKind, &'static str, &'static str);

const TREE: &[TreeEntry] = &[
    ("my-portfolio", FileKind::Directory, "", ""),
    ("assets", FileKind::Directory, "", "├── "),
    ("hero.png", FileKind::Regular, "│   ", "├── "),
    ("fonts.zip", FileKind::Regular, "│   ", "└── "),
    ("src", FileKind::Directory, "", "├── "),
    ("index.ts", FileKind::Regular, "│   ", "├── "),
    ("theme.ts", FileKind::Regular, "│   ", "└── "),
    // Symlink entry — lights up FileSymlink → cyan. The arrow and target
    // stay muted so the eye reads "docs" as the primary, coloured ref.
    ("docs", FileKind::Symlink, "", "├── "),
    ("README.md", FileKind::Regular, "", "├── "),
    ("package.json", FileKind::Regular, "", "├── "),
    (".gitignore", FileKind::Regular, "", "├── "),
    ("deploy", FileKind::Executable, "", "└── "),
];

/// Tree block. Realises slots 1, 2, 3, 4, 5, 6, 8.
pub fn render_tree_block(palette: &Palette) -> String {
    let muted = palette.resolve(SemanticColor::Muted); // slot 8

    let mut out = String::with_capacity(512);

    for (name, kind, indent, prefix) in TREE {
        // Box-drawing chars always muted (structural, not content).
        if !indent.is_empty() || !prefix.is_empty() {
            span(&mut out, &muted, &format!("{indent}{prefix}"));
        }

        // Primary name in its classified role.
        let role = classify(name, *kind);
        let color = palette.resolve(role);
        span(&mut out, &color, name);

        // Symlinks carry an arrow + target, both muted so the name pops.
        if *kind == FileKind::Symlink {
            span(&mut out, &muted, " -> ../shared/docs");
        }

        out.push('\n');
    }

    out
}

/// Git-log block (ASCII graph with one merge). Realises slots 4, 6, 7, 8, 9,
/// 11, 12, 14, 15.
pub fn render_git_log_block(palette: &Palette) -> String {
    let accent = palette.resolve(SemanticColor::Accent); // slot 6 — graph `*` / `│`
    let num = palette.resolve(SemanticColor::Number); // slot 3 — normal hashes
    let muted = palette.resolve(SemanticColor::Muted); // slot 8 — punctuation
    let branch = palette.resolve(SemanticColor::GitBranch); // slot 4 — main
    let text = palette.resolve(SemanticColor::Text); // default — subject
    let white = &palette.white; // slot 7 — " · 2h" suffix
    let bright_red = &palette.bright_red; // slot 9 — [mm] chip
    let bright_yellow = &palette.bright_yellow; // slot 11 — merge hash
    let bright_blue = &palette.bright_blue; // slot 12 — origin/main
    let bright_cyan = &palette.bright_cyan; // slot 14 — merge glyphs
    let bright_white = &palette.bright_white; // slot 15 — HEAD token

    let mut out = String::with_capacity(1024);

    // ── Line 1: HEAD commit with full decoration + suffix + author chip.
    // * a3f2c1d (HEAD -> main, origin/main) feat: seal palette · 2h [mm]
    span(&mut out, &accent, "*");
    out.push(' ');
    span(&mut out, &num, "a3f2c1d");
    out.push(' ');
    span(&mut out, &muted, "(");
    span(&mut out, bright_white, "HEAD");
    span(&mut out, &muted, " -> ");
    span(&mut out, &branch, "main");
    span(&mut out, &muted, ", ");
    span(&mut out, bright_blue, "origin/main");
    span(&mut out, &muted, ")");
    out.push(' ');
    span(&mut out, &text, "feat: seal palette");
    span(&mut out, white, " · 2h");
    out.push(' ');
    span(&mut out, bright_red, "[mm]");
    out.push('\n');

    // ── Line 2: * 8b0e7f2 fix: normalize shell quoting in shared env
    span(&mut out, &accent, "*");
    out.push(' ');
    span(&mut out, &num, "8b0e7f2");
    out.push(' ');
    span(
        &mut out,
        &text,
        "fix: normalize shell quoting in shared env",
    );
    out.push('\n');

    // ── Line 3: * 4d91a3e Merge branch 'demo-showcase'
    span(&mut out, &accent, "*");
    out.push_str("   ");
    span(&mut out, bright_yellow, "4d91a3e");
    out.push(' ');
    span(&mut out, &text, "Merge branch 'demo-showcase'");
    out.push('\n');

    // ── Line 4: │╲ (merge-open glyphs)
    span(&mut out, bright_cyan, "│╲");
    out.push('\n');

    // ── Line 5: │ * 6e2b1c8 docs: add README screenshot
    span(&mut out, &accent, "│");
    out.push(' ');
    span(&mut out, &accent, "*");
    out.push(' ');
    span(&mut out, &num, "6e2b1c8");
    out.push(' ');
    span(&mut out, &text, "docs: add README screenshot");
    out.push('\n');

    // ── Line 6: │╱ (merge-close glyphs)
    span(&mut out, bright_cyan, "│╱");
    out.push('\n');

    // ── Line 7: * 2c7a9fe chore: bump crossterm to 0.27
    span(&mut out, &accent, "*");
    out.push(' ');
    span(&mut out, &num, "2c7a9fe");
    out.push(' ');
    span(&mut out, &text, "chore: bump crossterm to 0.27");
    out.push('\n');

    out
}

/// Progress block (single line). Realises slots 0, 2, 6, 8, 10.
/// Layout: `<chip><label> <bar><spaces> <percent>`
/// - 2 (chip) + 7 (label) + 3 + 28 (filled) + 1 (partial) + 11 (empty)
/// + 3 + 3 = 58 cols.
pub fn render_progress_block(palette: &Palette) -> String {
    let black = &palette.black; // slot 0 — status chip
    let muted = palette.resolve(SemanticColor::Muted); // slot 8 — label
    let success = palette.resolve(SemanticColor::Success); // slot 2 — filled bar
    let bright_green = &palette.bright_green; // slot 10 — leading edge
    let accent = palette.resolve(SemanticColor::Accent); // slot 6 — 72%

    let mut out = String::with_capacity(256);

    // Leading status chip (■ + trailing space) — slot 0.
    span(&mut out, black, "■ ");

    // Label "Brewing" — slot 8.
    span(&mut out, &muted, "Brewing");

    // 3 spaces before the bar.
    out.push_str("   ");

    // Filled body: 28 × █ — slot 2.
    let filled: String = "█".repeat(28);
    span(&mut out, &success, &filled);

    // Leading-edge partial glyph — slot 10.
    span(&mut out, bright_green, "▊");

    // Empty portion: 11 spaces (implicit foreground).
    out.push_str("           ");

    // 3 spaces before the percent.
    out.push_str("   ");

    // 72% — slot 6.
    span(&mut out, &accent, "72%");

    out.push('\n');

    out
}

/// palette swatch for picker preview.
/// `full=false` (mini-preview mode): 1 line, 8 background cells rendering
/// the 8 "normal" ANSI slots (black..white). Each cell is 3 spaces wide
/// (total 24 visible cols + the trailing newline).
/// `full=true` (Tab full-preview mode): 2 lines.
/// * Line 1: 16 background cells — all 8 "normal" slots (black..white)
/// followed by the 8 "bright" slots (bright_black..bright_white). Each
/// cell is 5 spaces wide (total 80 visible cols).
/// * Line 2: 8 named labels `rosewater red peach yellow green sky blue
/// mauve` rendered in `palette.foreground`. Labels are 10 cols wide
/// each so they sit under pairs of cells from line 1 (2×5 = 10 per
/// label). The 10-col width fits the longest canonical label
/// (`rosewater`, 9 chars) with a trailing space separator — the
/// previous 8-col width silently overflowed and merged "rosewater"
/// with "red" (WR-01). The names are Catppuccin's canonical 8-accent
/// family — reused across all 18 themes for consistency per sketch
/// 005 A /.
/// The returned String is pure (no I/O, no panics on bad palette bytes
/// `fg` degrades to empty). Consumed by `preview::compose`.
// SWATCH-RENDERER: intentionally raw ANSI (renders palette colors, not role text)
pub fn render_palette_swatch(palette: &Palette, full: bool) -> String {
    // Background-cell helper: emits `ESC[48;2;R;G;B m` + `width` spaces + RESET.
    // On malformed palette hex (a theme-file bug, not a user-facing error)
    // we degrade to `width` uncolored spaces so layout stays stable.
    let push_cell =
        |out: &mut String, hex: &str, width: usize| match PaletteRenderer::hex_to_rgb(hex) {
            Ok((r, g, b)) => {
                out.push_str(&format!("\x1b[48;2;{r};{g};{b}m"));
                for _ in 0..width {
                    out.push(' ');
                }
                out.push_str(RESET);
            }
            Err(_) => {
                for _ in 0..width {
                    out.push(' ');
                }
            }
        };

    if !full {
        let mut out = String::with_capacity(128);
        // Mini-mode: 8 cells × 3 spaces = 24 cols (normal slots 0–7).
        for hex in [
            palette.black.as_str(),
            palette.red.as_str(),
            palette.green.as_str(),
            palette.yellow.as_str(),
            palette.blue.as_str(),
            palette.magenta.as_str(),
            palette.cyan.as_str(),
            palette.white.as_str(),
        ] {
            push_cell(&mut out, hex, 3);
        }
        out.push('\n');
        out
    } else {
        let mut out = String::with_capacity(512);
        // Full-mode line 1: 16 cells × 5 spaces = 80 cols.
        for hex in [
            palette.black.as_str(),
            palette.red.as_str(),
            palette.green.as_str(),
            palette.yellow.as_str(),
            palette.blue.as_str(),
            palette.magenta.as_str(),
            palette.cyan.as_str(),
            palette.white.as_str(),
            palette.bright_black.as_str(),
            palette.bright_red.as_str(),
            palette.bright_green.as_str(),
            palette.bright_yellow.as_str(),
            palette.bright_blue.as_str(),
            palette.bright_magenta.as_str(),
            palette.bright_cyan.as_str(),
            palette.bright_white.as_str(),
        ] {
            push_cell(&mut out, hex, 5);
        }
        out.push('\n');

        // Full-mode line 2: 8 × 10-col labels in `palette.foreground`
        // (Catppuccin canonical 8 accents — shared across all themes).
        // WR-01 fix: label cells must be wide enough to fit the longest
        // canonical name (`rosewater`, 9 chars) without overflowing into
        // the next cell. 10 cols also keeps the row aligned 1:1 with
        // pairs of the 5-col bg cells above (2 × 5 = 10 per label).
        const NAMES: [&str; 8] = [
            "rosewater",
            "red",
            "peach",
            "yellow",
            "green",
            "sky",
            "blue",
            "mauve",
        ];
        out.push_str(&fg(&palette.foreground));
        for name in NAMES {
            out.push_str(&format!("{name:<10}"));
        }
        out.push_str(RESET);
        out.push('\n');
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::ThemeRegistry;

    fn mocha_palette() -> Palette {
        let registry = ThemeRegistry::new().expect("registry");
        registry
            .get("catppuccin-mocha")
            .expect("catppuccin-mocha must exist")
            .palette
            .clone()
    }

    /// Concatenate the 4 block renderers in the canonical demo order so the
    /// migrated D-B4 tests can keep asserting against the combined output
    /// without a shared `render_to_string` helper (which was NOT migrated
    /// compose.rs is its replacement).
    fn render_all_blocks(palette: &Palette) -> String {
        let mut out = String::with_capacity(4096);
        out.push_str(&render_code_block(palette));
        out.push('\n');
        out.push_str(&render_tree_block(palette));
        out.push('\n');
        out.push_str(&render_git_log_block(palette));
        out.push('\n');
        out.push_str(&render_progress_block(palette));
        out.push('\n');
        out
    }

    /// Strip ANSI CSI sequences so visible-width assertions are accurate.
    fn strip_ansi(s: &str) -> String {
        let mut out = String::with_capacity(s.len());
        let mut iter = s.chars().peekable();
        while let Some(c) = iter.next() {
            if c == '\x1b' && iter.peek() == Some(&'[') {
                iter.next();
                while let Some(&c) = iter.peek() {
                    iter.next();
                    if c == 'm' {
                        break;
                    }
                }
                continue;
            }
            out.push(c);
        }
        out
    }

    /// Collect every distinct RGB triplet emitted as a truecolor foreground
    /// (ESC `[` `3` `8` `;` `2` `;` R `;` G `;` B `m`) by the render output.
    /// The prefix is built from bytes so the test source does not itself
    /// contain the raw styling escape literal that the Wave-5 grep gate
    /// scans for.
    fn collected_fg_triplets(out: &str) -> std::collections::HashSet<(u8, u8, u8)> {
        let mut triplets = std::collections::HashSet::new();
        let prefix_bytes: [u8; 7] = [0x1b, b'[', b'3', b'8', b';', b'2', b';'];
        let prefix = std::str::from_utf8(&prefix_bytes).unwrap();
        let mut idx = 0;
        while let Some(pos) = out[idx..].find(prefix) {
            let start = idx + pos + prefix.len();
            if let Some(end) = out[start..].find('m') {
                let body = &out[start..start + end];
                let parts: Vec<&str> = body.split(';').collect();
                if parts.len() == 3 {
                    if let (Ok(r), Ok(g), Ok(b)) = (
                        parts[0].parse::<u8>(),
                        parts[1].parse::<u8>(),
                        parts[2].parse::<u8>(),
                    ) {
                        triplets.insert((r, g, b));
                    }
                }
                idx = start + end;
            } else {
                break;
            }
        }
        triplets
    }

    #[test]
    fn render_to_string_emits_ansi_24bit_fg() {
        let out = render_all_blocks(&mocha_palette());
        // Byte-slice probe for the truecolor-fg SGR prefix (`ESC [ 3 8 ; 2`)
        // avoids a literal escape in the test source so the Wave-5 grep
        // gate stays authoritative (same shape as Wave-3's
        // `status_panel_preserves_palette_swatch`).
        let bytes = out.as_bytes();
        assert!(
            bytes
                .windows(6)
                .any(|w| w == [0x1b, b'[', b'3', b'8', b';', b'2']),
            "output must contain ANSI 24-bit foreground escape"
        );
    }

    #[test]
    fn render_to_string_all_lines_fit_80_cols() {
        let out = render_all_blocks(&mocha_palette());
        for (i, line) in out.lines().enumerate() {
            let visible = strip_ansi(line);
            let width = visible.chars().count();
            assert!(
                width <= 80,
                "line {i} is {width} visible cols (>80): {visible:?}"
            );
        }
    }

    #[test]
    fn render_to_string_contains_all_four_blocks() {
        let out = render_all_blocks(&mocha_palette());
        let visible = strip_ansi(&out);
        assert!(visible.contains("type User"), "code block missing");
        assert!(visible.contains("my-portfolio"), "tree block missing");
        assert!(visible.contains("HEAD -> main"), "git-log block missing");
        assert!(visible.contains("72%"), "progress block missing");
    }

    /// D-B4 coverage gate (strict unit-level check).
    /// A single render must light up ALL 16 ANSI palette slots (normal 0–7 +
    /// bright 8–15). This is the sample-data design contract; drift here is a
    /// product regression, not a test flake. Integration test in
    /// enforces the same invariant end-to-end; keeping it at unit level too
    /// means wave 2 fails fast without waiting for wave 4.
    #[test]
    fn render_covers_all_ansi_slots() {
        let palette = mocha_palette();
        let out = render_all_blocks(&palette);
        let emitted = collected_fg_triplets(&out);

        let ansi_slots: [(&str, &str); 16] = [
            ("black", palette.black.as_str()),
            ("red", palette.red.as_str()),
            ("green", palette.green.as_str()),
            ("yellow", palette.yellow.as_str()),
            ("blue", palette.blue.as_str()),
            ("magenta", palette.magenta.as_str()),
            ("cyan", palette.cyan.as_str()),
            ("white", palette.white.as_str()),
            ("bright_black", palette.bright_black.as_str()),
            ("bright_red", palette.bright_red.as_str()),
            ("bright_green", palette.bright_green.as_str()),
            ("bright_yellow", palette.bright_yellow.as_str()),
            ("bright_blue", palette.bright_blue.as_str()),
            ("bright_magenta", palette.bright_magenta.as_str()),
            ("bright_cyan", palette.bright_cyan.as_str()),
            ("bright_white", palette.bright_white.as_str()),
        ];

        let mut missing: Vec<&str> = Vec::new();
        for (name, hex) in ansi_slots {
            let (r, g, b) = PaletteRenderer::hex_to_rgb(hex).expect("valid hex");
            if !emitted.contains(&(r, g, b)) {
                missing.push(name);
            }
        }

        assert!(
            missing.is_empty(),
            "D-B4 violated: {} of 16 ANSI slots not emitted in render. \
             Missing: {:?}. Emitted: {:?}",
            missing.len(),
            missing,
            emitted
        );

        // Sanity: with 16 distinct palette slots in Catppuccin Mocha there
        // should be exactly 16 distinct RGB triplets at minimum (more are
        // fine — SemanticColor may map extra slots too).
        assert!(
            emitted.len() >= 16,
            "D-B4: expected >=16 distinct RGB triplets, got {}",
            emitted.len()
        );
    }

    /// The palette showcase is the whole point of the 4-block renderer, so
    /// the render output MUST carry many `38;2;` 24-bit swatch escapes.
    /// Byte-slice probe avoids tripping the Wave-5 grep gate ourselves.
    #[test]
    fn demo_render_preserves_many_palette_swatches() {
        let out = render_all_blocks(&mocha_palette());
        let bytes = out.as_bytes();
        let needle: [u8; 6] = [0x1b, b'[', b'3', b'8', b';', b'2'];
        let swatch_count = bytes.windows(6).filter(|w| *w == needle).count();
        assert!(
            swatch_count >= 10,
            "render should emit >=10 palette swatch escapes, got {swatch_count}"
        );
    }

    /// Byte-slice probe helper for the 24-bit background-SGR prefix
    /// (`ESC [ 4 8 ; 2 ;`). Counts how many background cells the swatch
    /// rendered. Avoids a literal escape in the test source so the
    /// Wave-5 grep gate stays authoritative.
    fn count_bg_cells(out: &str) -> usize {
        let bytes = out.as_bytes();
        let needle: [u8; 7] = [0x1b, b'[', b'4', b'8', b';', b'2', b';'];
        bytes.windows(7).filter(|w| *w == needle).count()
    }

    /// mini-preview swatch is a single line of 8 background cells
    /// covering ANSI slots 0–7 (black..white).
    #[test]
    fn palette_swatch_mini_is_single_line() {
        let out = render_palette_swatch(&mocha_palette(), false);
        assert!(out.ends_with('\n'), "mini swatch must end with newline");
        // Exactly one line break — i.e. one trailing '\n', zero internal.
        assert_eq!(
            out.matches('\n').count(),
            1,
            "mini swatch must be a single line, got: {out:?}"
        );
        // 8 background cells (normal slots 0–7).
        assert_eq!(
            count_bg_cells(&out),
            8,
            "mini swatch must render 8 bg cells, got: {out:?}"
        );
        // Visible width (after ANSI strip) = 8 cells × 3 spaces = 24 cols.
        let visible = strip_ansi(&out);
        let body = visible.trim_end_matches('\n');
        assert_eq!(
            body.chars().count(),
            24,
            "mini swatch body must be 24 visible cols (8 × 3 spaces), got {body:?}"
        );
    }

    /// full-preview swatch is 2 lines — 16 cells on line 1 (slots
    /// 0–15), 8 Catppuccin canonical labels on line 2.
    #[test]
    fn palette_swatch_full_is_two_lines_with_names() {
        let out = render_palette_swatch(&mocha_palette(), true);
        // Exactly two line breaks — i.e. two '\n'.
        assert_eq!(
            out.matches('\n').count(),
            2,
            "full swatch must be 2 lines, got: {out:?}"
        );
        // 16 background cells (slots 0–15).
        assert_eq!(
            count_bg_cells(&out),
            16,
            "full swatch must render 16 bg cells, got: {out:?}"
        );
        // Label row must carry the 8 Catppuccin canonical names in order.
        let visible = strip_ansi(&out);
        let lines: Vec<&str> = visible.trim_end_matches('\n').split('\n').collect();
        assert_eq!(lines.len(), 2, "expected exactly 2 lines, got {lines:?}");
        let labels = lines[1];
        for name in [
            "rosewater",
            "red",
            "peach",
            "yellow",
            "green",
            "sky",
            "blue",
            "mauve",
        ] {
            assert!(
                labels.contains(name),
                "label row must contain {name:?}, got: {labels:?}"
            );
        }
        // Names must appear in the locked order.
        let positions: Vec<_> = [
            "rosewater",
            "red",
            "peach",
            "yellow",
            "green",
            "sky",
            "blue",
            "mauve",
        ]
        .iter()
        .map(|n| labels.find(n).unwrap_or(usize::MAX))
        .collect();
        let mut sorted = positions.clone();
        sorted.sort_unstable();
        assert_eq!(
            positions, sorted,
            " label order violated — expected rosewater red peach yellow green sky blue mauve, got positions {positions:?} in {labels:?}"
        );
    }

    /// snapshot: lock the full-mode structure (ANSI-stripped) for
    /// catppuccin-mocha so any drift in cell widths, label order, or
    /// spacing surfaces immediately. Strips ANSI first so the snapshot
    /// locks structure + names, not fragile byte sequences.
    #[test]
    fn palette_swatch_8_named_cells() {
        let out = render_palette_swatch(&mocha_palette(), true);
        let stripped = strip_ansi(&out);
        insta::assert_snapshot!("palette_swatch_8_named_cells", stripped);
    }
}
