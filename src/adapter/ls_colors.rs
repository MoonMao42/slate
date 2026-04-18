//! LS_COLORS + EZA_COLORS adapter — projects the Phase-15 file_type classifier
//! into two environment-variable strings.
//!
//! The rendered strings are materialised into the managed shell integration
//! files (`managed/shell/env.{zsh,bash,fish}`) by `SharedShellModel` in
//! Plan 16-04 (Wave 2). This module owns the projection only; it does not
//! write any files.
//!
//! ## File-type kind keys (LS_COLORS)
//!
//! Phase-15 `file_type_colors::classify` is the single source of truth; it
//! defines five `FileKind`s — `Regular`, `Directory`, `Symlink`, `Executable`
//! (plus `Hidden` derived at match-time). GNU `LS_COLORS`, however, defines
//! eight file-type kind keys that `ls` emits for filesystem entries beyond
//! those four. We reuse the closest Phase-15 role for each missing kind so
//! the output stays consistent with the project's single-palette invariant,
//! and document the intentional reuse below:
//!
//! | Key | Meaning                           | Role reused           |
//! |-----|-----------------------------------|-----------------------|
//! | `di`| Directory                         | `FileDir`             |
//! | `ln`| Symbolic link                     | `FileSymlink`         |
//! | `ex`| Executable file                   | `FileExec`            |
//! | `or`| Orphan symlink (broken)           | `FileSymlink` (reuse) |
//! | `so`| Unix socket                       | `FileExec` (reuse)    |
//! | `pi`| Named pipe / FIFO                 | `FileConfig` (reuse)  |
//! | `bd`| Block device                      | `FileConfig` (reuse)  |
//! | `cd`| Character device                  | `FileConfig` (reuse)  |
//!
//! `or` / `so` / `pi` / `bd` / `cd` do not appear in the Phase-15 classifier
//! (`FileKind` has no Pipe / Socket / BlockDevice / CharDevice variants).
//! Extending `FileKind` is deferred — until then, the closest semantically
//! adjacent role is emitted per RESEARCH §Pitfall 4.
//!
//! ## `reset:` sentinel on EZA_COLORS
//!
//! Per `eza_colors(5)`, eza merges `EZA_COLORS` on top of its built-in
//! extension → color map. Setting `EZA_COLORS="*.rs=…"` alone does not
//! reset eza's defaults; colors for extensions we do not override leak
//! through. To keep "one palette across the stack" true, `render_eza_colors`
//! prepends `reset:` — eza wipes its built-in DB on `reset` and then honours
//! only the palette-driven entries we emit (with `no=0` handling everything
//! else as default foreground). See RESEARCH §Pattern 2 lines 338-384.
//!
//! Note on `allow(dead_code)`: the `pub(crate)` render functions and the
//! `FILE_TYPE_KIND_KEYS` table are consumed by Plan 16-04 (`SharedShellModel`)
//! in Wave 2. Until that wave lands they have no non-test call sites; the
//! module-level `allow(dead_code)` is intentional and will be dropped when
//! Plan 16-04 wires `render_strings()` into shell-integration composition.

#![allow(dead_code)]

use crate::adapter::palette_renderer::PaletteRenderer;
use crate::adapter::{ApplyOutcome, ApplyStrategy, ToolAdapter};
use crate::cli::picker::preview_panel::SemanticColor;
use crate::design::file_type_colors::{extension_map, full_name_map};
use crate::env::SlateEnv;
use crate::error::Result;
use crate::theme::{Palette, ThemeVariant};
use std::fmt::Write;
use std::path::PathBuf;

/// GNU `LS_COLORS` file-type kind keys → Phase-15 SemanticColor roles.
///
/// Order is deterministic (same iteration order as the wire format).
/// See module-level docs for the intentional reuse rationale for `or`,
/// `so`, `pi`, `bd`, `cd`.
static FILE_TYPE_KIND_KEYS: &[(&str, SemanticColor)] = &[
    ("di", SemanticColor::FileDir),
    ("ln", SemanticColor::FileSymlink),
    ("ex", SemanticColor::FileExec),
    // Intentional reuse — no Phase-15 role exists for these kinds yet.
    ("or", SemanticColor::FileSymlink),
    ("so", SemanticColor::FileExec),
    ("pi", SemanticColor::FileConfig),
    ("bd", SemanticColor::FileConfig),
    ("cd", SemanticColor::FileConfig),
];

/// Resolve a `SemanticColor` role into the inner `38;2;R;G;B` substring used
/// by `LS_COLORS` / `EZA_COLORS` entries. On hex-parse error (should be
/// unreachable for validated palettes) we degrade to the reset literal `"0"`
/// rather than crash — a single malformed hex must not poison the whole
/// env-var string.
fn ansi_code(palette: &Palette, role: SemanticColor) -> String {
    let hex = palette.resolve(role);
    match PaletteRenderer::hex_to_rgb(&hex) {
        Ok((r, g, b)) => PaletteRenderer::rgb_to_ansi_24bit(r, g, b),
        Err(_) => String::from("0"),
    }
}

/// Render LS_COLORS string for a given palette.
///
/// Layout: `rs=0:no=0:<file-type kinds>:<*.ext entries>:<FULL_NAME entries>`.
/// Every per-entry colour is a truecolor `38;2;R;G;B` triple (no 256-colour
/// fallback, no `\x1b[` prefix, no trailing `m`).
pub(crate) fn render_ls_colors(palette: &Palette) -> String {
    let mut out = String::with_capacity(1024);
    out.push_str("rs=0:no=0");

    for (key, role) in FILE_TYPE_KIND_KEYS {
        // write! on String cannot fail — unwrap is correctness-preserving.
        let _ = write!(out, ":{key}={}", ansi_code(palette, *role));
    }

    for (ext, role) in extension_map() {
        let _ = write!(out, ":*.{ext}={}", ansi_code(palette, *role));
    }

    for (name, role) in full_name_map() {
        let _ = write!(out, ":{name}={}", ansi_code(palette, *role));
    }

    out
}

/// Render EZA_COLORS string for a given palette.
///
/// Layout: `reset:<same body as render_ls_colors>:<eza identity keys>`.
/// Prepending `reset` is required to stop eza's built-in extension map from
/// leaking colours that aren't palette-sourced (see module docs).
pub(crate) fn render_eza_colors(palette: &Palette) -> String {
    let mut out = String::with_capacity(1024);
    out.push_str("reset");

    for (key, role) in FILE_TYPE_KIND_KEYS {
        let _ = write!(out, ":{key}={}", ansi_code(palette, *role));
    }

    for (ext, role) in extension_map() {
        let _ = write!(out, ":*.{ext}={}", ansi_code(palette, *role));
    }

    for (name, role) in full_name_map() {
        let _ = write!(out, ":{name}={}", ansi_code(palette, *role));
    }

    // eza-specific identity keys (D-A4):
    //   uu / gu → current user + group  → Text (primary foreground emphasis)
    //   un / gn / da → other users / groups / dates → Muted (secondary metadata)
    let text = ansi_code(palette, SemanticColor::Text);
    let muted = ansi_code(palette, SemanticColor::Muted);
    let _ = write!(out, ":uu={text}");
    let _ = write!(out, ":gu={text}");
    let _ = write!(out, ":un={muted}");
    let _ = write!(out, ":gn={muted}");
    let _ = write!(out, ":da={muted}");

    out
}

/// Convenience tuple `(ls_colors, eza_colors)` — Plan 16-04 calls this from
/// `SharedShellModel::new` so the two env vars stay in lock-step.
pub(crate) fn render_strings(palette: &Palette) -> (String, String) {
    (render_ls_colors(palette), render_eza_colors(palette))
}

/// LS_COLORS / EZA_COLORS adapter.
///
/// Implements `ToolAdapter` so it participates in the standard registry
/// lifecycle (setup / apply-all iteration). The adapter's `apply_theme`
/// is a pure signal holder — actual string emission happens via
/// `render_strings()` called from shell-integration composition in
/// Plan 16-04.
pub struct LsColorsAdapter;

impl LsColorsAdapter {
    fn config_home() -> Result<PathBuf> {
        let env = SlateEnv::from_process()?;
        Ok(env.config_dir().to_path_buf())
    }
}

impl ToolAdapter for LsColorsAdapter {
    fn tool_name(&self) -> &'static str {
        "ls_colors"
    }

    fn is_installed(&self) -> Result<bool> {
        // Always applicable: writing `LS_COLORS` is a silent no-op on BSD
        // `ls` and takes effect the moment the user installs GNU coreutils
        // (D-B5). Never gated.
        Ok(true)
    }

    fn integration_config_path(&self) -> Result<PathBuf> {
        // The "integration" file is the managed shell env script; the env
        // vars are exported from there into every new shell.
        Ok(Self::config_home()?
            .join("managed")
            .join("shell")
            .join("env.zsh"))
    }

    fn managed_config_path(&self) -> PathBuf {
        match SlateEnv::from_process() {
            Ok(env) => env.config_dir().join("managed").join("shell"),
            Err(_) => PathBuf::from(".config/slate/managed/shell"),
        }
    }

    fn apply_strategy(&self) -> ApplyStrategy {
        ApplyStrategy::EnvironmentVariable
    }

    fn apply_theme(&self, _theme: &ThemeVariant) -> Result<ApplyOutcome> {
        // String emission is owned by Plan 16-04's `SharedShellModel::new`,
        // which calls `render_strings(&palette)` during shell-integration
        // composition. This adapter's job is purely to declare the
        // `requires_new_shell` signal honestly (D-C3 / Pitfall from 16-CONTEXT
        // D-C4 — env vars only materialise in a fresh shell).
        Ok(ApplyOutcome::Applied {
            requires_new_shell: true,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::catppuccin::catppuccin_mocha;
    use crate::theme::gruvbox::gruvbox_dark;
    use regex::Regex;
    use rstest::rstest;

    fn test_palettes() -> Vec<(&'static str, Palette)> {
        vec![
            (
                "catppuccin_mocha",
                catppuccin_mocha().expect("mocha loads").palette,
            ),
            (
                "gruvbox_dark",
                gruvbox_dark().expect("gruvbox-dark loads").palette,
            ),
        ]
    }

    // --- LS_COLORS tests ---

    #[rstest]
    fn ls_colors_starts_with_rs_and_no_sentinels() {
        for (label, palette) in test_palettes() {
            let out = render_ls_colors(&palette);
            assert!(
                out.starts_with("rs=0:no=0"),
                "palette={label}: string did not start with `rs=0:no=0:`; got={out}",
            );
        }
    }

    #[rstest]
    fn ls_colors_contains_all_file_type_kind_keys() {
        let code = Regex::new(r"=38;2;\d+;\d+;\d+").expect("regex");
        for (label, palette) in test_palettes() {
            let out = render_ls_colors(&palette);
            for key in ["di", "ln", "ex", "or", "so", "pi", "bd", "cd"] {
                let needle = format!(":{key}");
                let pos = out
                    .find(&needle)
                    .unwrap_or_else(|| panic!("palette={label}: missing kind key `{key}`"));
                let tail = &out[pos + needle.len()..];
                assert!(
                    code.is_match(tail),
                    "palette={label}: kind key `{key}` not followed by truecolor code (tail={tail})",
                );
            }
        }
    }

    #[rstest]
    fn ls_colors_contains_every_extension_map_entry() {
        for (label, palette) in test_palettes() {
            let out = render_ls_colors(&palette);
            for (ext, _role) in extension_map() {
                let needle = format!(":*.{ext}=38;2;");
                assert!(
                    out.contains(&needle),
                    "palette={label}: missing extension entry `{needle}`",
                );
            }
        }
    }

    #[rstest]
    fn ls_colors_contains_every_full_name_entry() {
        for (label, palette) in test_palettes() {
            let out = render_ls_colors(&palette);
            for (name, _role) in full_name_map() {
                let needle = format!(":{name}=38;2;");
                assert!(
                    out.contains(&needle),
                    "palette={label}: missing full-name entry `{needle}`",
                );
            }
        }
    }

    #[rstest]
    fn ls_colors_round_trips_through_classifier() {
        use crate::design::file_type_colors::{classify, FileKind};

        // Curated sample: the filename's classified role must equal the ANSI
        // code we emit in the LS_COLORS entry that `ls` will use for it.
        let cases: &[(&str, FileKind, &str)] = &[
            // (filename, kind, lookup-prefix used to locate the entry in LS_COLORS)
            ("main.rs", FileKind::Regular, ":*.rs="),
            ("README.md", FileKind::Regular, ":*.md="),
            ("photo.png", FileKind::Regular, ":*.png="),
            ("Cargo.lock", FileKind::Regular, ":Cargo.lock="),
            ("pnpm-lock.yaml", FileKind::Regular, ":pnpm-lock.yaml="),
            ("src", FileKind::Directory, ":di="),
            ("deploy", FileKind::Executable, ":ex="),
            ("link", FileKind::Symlink, ":ln="),
        ];

        for (palette_label, palette) in test_palettes() {
            let out = render_ls_colors(&palette);
            for (name, kind, prefix) in cases {
                let role = classify(name, *kind);
                let expected_ansi = ansi_code(&palette, role);

                let pos = out.find(prefix).unwrap_or_else(|| {
                    panic!(
                        "palette={palette_label}: entry for `{name}` via prefix `{prefix}` not found",
                    )
                });
                // Extract the ANSI substring between "<prefix>" and next ":"
                // (or end of string).
                let value_start = pos + prefix.len();
                let value_end = out[value_start..]
                    .find(':')
                    .map(|n| value_start + n)
                    .unwrap_or(out.len());
                let actual_ansi = &out[value_start..value_end];
                assert_eq!(
                    actual_ansi, expected_ansi,
                    "palette={palette_label}: round-trip mismatch for `{name}` \
                     (kind={kind:?}) — classifier→role→ansi `{expected_ansi}` \
                     != LS_COLORS entry `{actual_ansi}`",
                );
            }
        }
    }

    #[rstest]
    fn ls_colors_uses_only_truecolor_codes() {
        // Every value between `=` and the next `:` (or EOS) must match
        // either a truecolor triple or the literal reset "0". No 256-colour
        // fallback, no wrapped escapes.
        let truecolor_or_reset =
            Regex::new(r"^(?:38;2;\d{1,3};\d{1,3};\d{1,3}|0)$").expect("regex");

        for (label, palette) in test_palettes() {
            let out = render_ls_colors(&palette);

            assert!(
                !out.contains("38;5;"),
                "palette={label}: 256-colour fallback detected (38;5;)",
            );
            assert!(
                !out.contains("\x1b["),
                "palette={label}: ANSI wrapper escape detected (\\x1b[)",
            );

            for entry in out.split(':') {
                // Each entry is either `key=value` or just `value` for the
                // reset sentinels at the head (`rs=0`, `no=0`). Validate the
                // RHS of `=` if present.
                let value = match entry.split_once('=') {
                    Some((_, v)) => v,
                    None => continue,
                };
                assert!(
                    truecolor_or_reset.is_match(value),
                    "palette={label}: non-truecolor value `{value}` in entry `{entry}`",
                );
            }
        }
    }

    #[rstest]
    fn ls_colors_or_uses_file_symlink_role_by_intent() {
        // Documents the intentional reuse — `or` (orphan symlink) shares
        // FileSymlink because classify() has no Orphan role.
        for (label, palette) in test_palettes() {
            let out = render_ls_colors(&palette);
            let symlink_ansi = ansi_code(&palette, SemanticColor::FileSymlink);
            let needle = format!(":or={symlink_ansi}");
            assert!(
                out.contains(&needle),
                "palette={label}: `or` did not reuse FileSymlink role (expected `{needle}`)",
            );
        }
    }

    // --- EZA_COLORS tests ---

    #[rstest]
    fn eza_colors_starts_with_reset_sentinel() {
        for (label, palette) in test_palettes() {
            let out = render_eza_colors(&palette);
            assert!(
                out.starts_with("reset:"),
                "palette={label}: eza string did not start with `reset:` (got={out})",
            );
            assert!(
                !out.starts_with(":reset"),
                "palette={label}: eza string must not have a leading colon before `reset`",
            );
        }
    }

    #[rstest]
    fn eza_colors_body_equals_ls_colors_body_for_shared_keys() {
        // The eza string and the ls string must carry identical entries for
        // the shared key-space (kind keys + extensions + full-name entries).
        for (label, palette) in test_palettes() {
            let ls = render_ls_colors(&palette);
            let eza = render_eza_colors(&palette);

            for (key, _role) in FILE_TYPE_KIND_KEYS {
                let ls_needle = format!(":{key}=");
                let ls_pos = ls.find(&ls_needle).expect("ls kind entry present");
                let ls_end = ls[ls_pos + 1..]
                    .find(':')
                    .map(|n| ls_pos + 1 + n)
                    .unwrap_or(ls.len());
                let ls_entry = &ls[ls_pos..ls_end];
                assert!(
                    eza.contains(ls_entry),
                    "palette={label}: eza body missing kind entry `{ls_entry}`",
                );
            }

            for (ext, _role) in extension_map() {
                let ls_needle = format!(":*.{ext}=");
                let ls_pos = ls.find(&ls_needle).expect("ls ext entry present");
                let ls_end = ls[ls_pos + 1..]
                    .find(':')
                    .map(|n| ls_pos + 1 + n)
                    .unwrap_or(ls.len());
                let ls_entry = &ls[ls_pos..ls_end];
                assert!(
                    eza.contains(ls_entry),
                    "palette={label}: eza body missing extension entry `{ls_entry}`",
                );
            }

            for (name, _role) in full_name_map() {
                let ls_needle = format!(":{name}=");
                let ls_pos = ls.find(&ls_needle).expect("ls full-name entry present");
                let ls_end = ls[ls_pos + 1..]
                    .find(':')
                    .map(|n| ls_pos + 1 + n)
                    .unwrap_or(ls.len());
                let ls_entry = &ls[ls_pos..ls_end];
                assert!(
                    eza.contains(ls_entry),
                    "palette={label}: eza body missing full-name entry `{ls_entry}`",
                );
            }
        }
    }

    #[rstest]
    fn eza_colors_has_identity_keys() {
        for (label, palette) in test_palettes() {
            let out = render_eza_colors(&palette);
            for key in ["uu", "gu", "un", "gn", "da"] {
                let needle = format!(":{key}=38;2;");
                assert!(
                    out.contains(&needle),
                    "palette={label}: eza missing identity key `{key}` (expected `{needle}`)",
                );
            }
        }
    }

    #[rstest]
    fn eza_colors_identity_keys_map_to_text_and_muted() {
        for (label, palette) in test_palettes() {
            let out = render_eza_colors(&palette);
            let text_ansi = ansi_code(&palette, SemanticColor::Text);
            let muted_ansi = ansi_code(&palette, SemanticColor::Muted);

            for key in ["uu", "gu"] {
                let needle = format!(":{key}={text_ansi}");
                assert!(
                    out.contains(&needle),
                    "palette={label}: `{key}` must carry Text colour (expected `{needle}`)",
                );
            }
            for key in ["un", "gn", "da"] {
                let needle = format!(":{key}={muted_ansi}");
                assert!(
                    out.contains(&needle),
                    "palette={label}: `{key}` must carry Muted colour (expected `{needle}`)",
                );
            }
        }
    }

    // --- Adapter contract tests ---

    #[test]
    fn ls_colors_adapter_declares_env_var_strategy() {
        let adapter = LsColorsAdapter;
        assert_eq!(adapter.tool_name(), "ls_colors");
        assert_eq!(adapter.apply_strategy(), ApplyStrategy::EnvironmentVariable);
    }

    #[test]
    fn ls_colors_adapter_apply_theme_requires_new_shell() {
        let adapter = LsColorsAdapter;
        let theme = catppuccin_mocha().expect("mocha loads");
        let outcome = adapter.apply_theme(&theme).expect("apply_theme ok");
        assert_eq!(
            outcome,
            ApplyOutcome::Applied {
                requires_new_shell: true,
            },
            "LsColorsAdapter must declare requires_new_shell: true per D-C3",
        );
    }

    #[test]
    fn ls_colors_adapter_is_installed_is_always_true() {
        // D-B5: env var is a silent no-op on BSD ls; never gate writes.
        let adapter = LsColorsAdapter;
        assert!(adapter.is_installed().expect("is_installed ok"));
    }

    #[test]
    fn render_strings_returns_both_env_vars() {
        let palette = catppuccin_mocha().expect("mocha loads").palette;
        let (ls, eza) = render_strings(&palette);
        assert_eq!(ls, render_ls_colors(&palette));
        assert_eq!(eza, render_eza_colors(&palette));
    }
}
