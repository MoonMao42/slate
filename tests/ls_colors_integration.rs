//! Phase 16 Plan 07 — Task 1: end-to-end integration tests for the
//! `LS_COLORS` / `EZA_COLORS` shell-integration pipeline.
//!
//! Scope: builds the full managed shell-integration file set via the
//! public `ConfigManager::write_shell_integration_file` entry point and
//! asserts that the generated `env.{zsh,fish}` files:
//!
//! * carry both `LS_COLORS` and `EZA_COLORS` with the correct shell-specific
//!   export syntax (POSIX `export X='…'` vs fish `set -gx X '…'`);
//! * use 24-bit truecolor escapes (`38;2;R;G;B`) and never fall back to the
//!   256-colour form (`38;5;…`) — D-A5, regression guard for Pitfall 3;
//! * round-trip through the Phase-15 file-type classifier — the ANSI code
//!   under `:*.rs=…` matches `PaletteRenderer::rgb_to_ansi_24bit(palette
//!   .resolve(classify("main.rs", FileKind::Regular)))`;
//! * pass `zsh -n` parsing when `zsh` is available on PATH (skips gracefully
//!   otherwise — guards Pitfall 5, shell-quoting regressions).
//!
//! Tests deliberately target the full managed-file pipeline rather than the
//! pure rendering functions (which are `pub(crate)` to `ls_colors.rs`); this
//! lets us catch shell-quoting and wiring regressions a unit test could not.

use slate_cli::adapter::palette_renderer::PaletteRenderer;
use slate_cli::cli::picker::preview_panel::SemanticColor;
use slate_cli::config::ConfigManager;
use slate_cli::design::file_type_colors::{classify, extension_map, FileKind};
use slate_cli::env::SlateEnv;
use slate_cli::theme::{Palette, ThemeRegistry};
use tempfile::TempDir;

/// Canonical dev theme — matches `SharedShellModel`'s sample theme in the
/// unit test suite. High-contrast palette makes the 38;2 assertions obvious.
const TEST_THEME_ID: &str = "catppuccin-mocha";

/// Set up a sandboxed `ConfigManager` + `ThemeVariant` and invoke the real
/// `write_shell_integration_file` path. Returns (tempdir, config, palette)
/// so callers can read the generated files and compare ANSI codes.
///
/// The tempdir is returned so it stays alive for the test lifetime — dropping
/// it would delete the files mid-assertion.
fn render_managed_shell_files() -> (TempDir, ConfigManager, Palette) {
    let tempdir = TempDir::new().expect("create tempdir");
    let env = SlateEnv::with_home(tempdir.path().to_path_buf());
    let config = ConfigManager::with_env(&env).expect("ConfigManager init");

    let registry = ThemeRegistry::new().expect("ThemeRegistry init");
    let theme = registry
        .get(TEST_THEME_ID)
        .unwrap_or_else(|| panic!("theme {TEST_THEME_ID} must be registered"))
        .clone();

    // Pin a nerd font so `should_prefer_plain_starship` returns a deterministic
    // value across CI hosts — without this the test would fork based on whether
    // the host has a Nerd Font installed.
    config
        .set_current_font("JetBrainsMono Nerd Font")
        .expect("pin test font");

    let palette = theme.palette.clone();
    config
        .write_shell_integration_file(&theme)
        .expect("write managed shell integration");

    (tempdir, config, palette)
}

/// Read `env.zsh` from the managed shell directory.
fn read_env_zsh(config: &ConfigManager) -> String {
    let path = config.managed_dir("shell").join("env.zsh");
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read env.zsh at {}: {e}", path.display()))
}

/// Read `env.fish` from the managed shell directory.
fn read_env_fish(config: &ConfigManager) -> String {
    let path = config.managed_dir("shell").join("env.fish");
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read env.fish at {}: {e}", path.display()))
}

/// Extract the value between the first pair of single quotes in `line`.
///
/// Used to lift the raw `LS_COLORS` / `EZA_COLORS` body out of a full
/// `export LS_COLORS='…'` or `set -gx LS_COLORS '…'` line so we can
/// assert on its structure.
fn single_quoted_value(line: &str) -> &str {
    let start = line
        .find('\'')
        .unwrap_or_else(|| panic!("no opening single-quote in line: {line}"));
    let rest = &line[start + 1..];
    let end = rest
        .rfind('\'')
        .unwrap_or_else(|| panic!("no closing single-quote in line: {line}"));
    &rest[..end]
}

/// Find the first line of `content` that starts with `prefix` (after trimming
/// left whitespace). Panics with a useful message if none match — the tests
/// care about presence, not iteration order.
fn first_line_starting_with<'a>(content: &'a str, prefix: &str) -> &'a str {
    content
        .lines()
        .find(|l| l.trim_start().starts_with(prefix))
        .unwrap_or_else(|| panic!("no line starts with `{prefix}` in:\n{content}"))
}

#[test]
fn env_zsh_contains_ls_colors_and_eza_colors_exports() {
    let (_temp, config, _palette) = render_managed_shell_files();
    let content = read_env_zsh(&config);

    let ls_line = first_line_starting_with(&content, "export LS_COLORS=");
    let eza_line = first_line_starting_with(&content, "export EZA_COLORS=");

    let ls_value = single_quoted_value(ls_line);
    let eza_value = single_quoted_value(eza_line);

    // LS_COLORS must open with the GNU reset / normal sentinels (D-A3).
    assert!(
        ls_value.starts_with("rs=0:no=0:"),
        "LS_COLORS body does not start with `rs=0:no=0:` (got `{ls_value}`)",
    );

    // EZA_COLORS must open with `reset:` so eza's built-in extension map is
    // cleared before our palette entries land (D-A4, RESEARCH §Pattern 2).
    assert!(
        eza_value.starts_with("reset:"),
        "EZA_COLORS body does not start with `reset:` (got `{eza_value}`)",
    );

    // Both bodies must carry at least one 24-bit truecolor escape — the
    // core payload of the feature.
    let truecolor = regex::Regex::new(r"38;2;\d{1,3};\d{1,3};\d{1,3}").expect("truecolor regex");
    assert!(
        truecolor.is_match(ls_value),
        "LS_COLORS body has no 38;2;R;G;B escape (got `{ls_value}`)",
    );
    assert!(
        truecolor.is_match(eza_value),
        "EZA_COLORS body has no 38;2;R;G;B escape (got `{eza_value}`)",
    );

    // Anti-pattern sweep — no 256-colour regressions (D-A5).
    assert!(
        !ls_value.contains("38;5;"),
        "LS_COLORS body contains forbidden 256-colour escape",
    );
    assert!(
        !eza_value.contains("38;5;"),
        "EZA_COLORS body contains forbidden 256-colour escape",
    );
}

#[test]
fn env_fish_uses_set_gx_for_ls_eza_colors() {
    let (_temp, config, _palette) = render_managed_shell_files();
    let content = read_env_fish(&config);

    let ls_line = first_line_starting_with(&content, "set -gx LS_COLORS ");
    let eza_line = first_line_starting_with(&content, "set -gx EZA_COLORS ");

    // Fish-specific: single-quoted value follows `set -gx NAME `. Same
    // `single_quoted_value` extractor works because the quoting shape is
    // identical to the POSIX line.
    let ls_value = single_quoted_value(ls_line);
    let eza_value = single_quoted_value(eza_line);

    assert!(
        ls_value.starts_with("rs=0:no=0:"),
        "fish LS_COLORS body mis-prefixed (got `{ls_value}`)",
    );
    assert!(
        eza_value.starts_with("reset:"),
        "fish EZA_COLORS body mis-prefixed (got `{eza_value}`)",
    );

    // Fish must not emit POSIX `export` syntax for these two vars. A file-wide
    // sweep catches stray `export LS_COLORS=` / `export EZA_COLORS=` lines that
    // could slip in if a future refactor forgets the fish branch.
    assert!(
        !content.contains("export LS_COLORS"),
        "env.fish must not emit `export LS_COLORS` (POSIX-only syntax)",
    );
    assert!(
        !content.contains("export EZA_COLORS"),
        "env.fish must not emit `export EZA_COLORS` (POSIX-only syntax)",
    );

    // Per D-A5 — fish side must also be truecolor only.
    assert!(
        !ls_value.contains("38;5;"),
        "fish LS_COLORS body contains forbidden 256-colour escape",
    );
    assert!(
        !eza_value.contains("38;5;"),
        "fish EZA_COLORS body contains forbidden 256-colour escape",
    );
}

#[test]
fn env_zsh_passes_shell_syntax_check() {
    // Skip if zsh isn't available (CI hosts without it). The assertion is
    // tool-gated, not #[ignore]'d — so on dev machines it runs by default.
    let zsh = match slate_cli::detection::command_path("zsh") {
        Some(path) => path,
        None => {
            println!(
                "zsh not available on PATH — skipping env.zsh syntax check \
                 (see RESEARCH §Pitfall 5)"
            );
            return;
        }
    };

    let (_temp, config, _palette) = render_managed_shell_files();
    let env_zsh_path = config.managed_dir("shell").join("env.zsh");

    // `zsh -n <path>` parses the file without executing it. Exit status 0
    // means quoting and command syntax round-tripped cleanly.
    let output = std::process::Command::new(&zsh)
        .arg("-n")
        .arg(&env_zsh_path)
        .output()
        .expect("spawn zsh -n");

    assert!(
        output.status.success(),
        "zsh -n failed on generated env.zsh at {}\nstderr:\n{}",
        env_zsh_path.display(),
        String::from_utf8_lossy(&output.stderr),
    );
}

#[test]
fn env_zsh_round_trips_classifier() {
    // Strongest guard on the feature: for every extension in the Phase-15
    // classifier map, the ANSI code printed inside LS_COLORS must equal the
    // ANSI code that `classify(name, Regular) → palette.resolve → rgb_to_ansi`
    // produces. If any of those steps drifts, the visible palette breaks.
    let (_temp, config, palette) = render_managed_shell_files();
    let content = read_env_zsh(&config);
    let ls_line = first_line_starting_with(&content, "export LS_COLORS=");
    let ls_value = single_quoted_value(ls_line);

    for (ext, _role) in extension_map() {
        // Build a plausible filename for the classifier. `FileKind::Regular`
        // is correct for all `extension_map()` entries (they're by-extension,
        // not by kind).
        let name = format!("fixture.{ext}");
        let role = classify(&name, FileKind::Regular);
        let expected = ansi_code_for_role(&palette, role);

        let needle = format!(":*.{ext}=");
        let pos = ls_value.find(&needle).unwrap_or_else(|| {
            panic!(
                "LS_COLORS body missing entry for `*.{ext}` — round-trip \
                 would fail at runtime (body={ls_value})",
            )
        });
        let value_start = pos + needle.len();
        let value_end = ls_value[value_start..]
            .find(':')
            .map(|n| value_start + n)
            .unwrap_or(ls_value.len());
        let actual = &ls_value[value_start..value_end];

        assert_eq!(
            actual, expected,
            "round-trip mismatch for `*.{ext}` via filename `{name}`: \
             classifier→`{expected}` vs LS_COLORS→`{actual}`",
        );
    }
}

/// Mirror of `ls_colors::ansi_code` — the adapter module keeps that helper
/// `pub(crate)`, so the integration test reimplements the 3-line pipeline
/// to avoid leaking a private API to external callers.
fn ansi_code_for_role(palette: &Palette, role: SemanticColor) -> String {
    let hex = palette.resolve(role);
    match PaletteRenderer::hex_to_rgb(&hex) {
        Ok((r, g, b)) => PaletteRenderer::rgb_to_ansi_24bit(r, g, b),
        Err(_) => String::from("0"),
    }
}
