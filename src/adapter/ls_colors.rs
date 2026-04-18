//! LS_COLORS + EZA_COLORS adapter — RED stub (tests fail, implementation pending).
//!
//! This module compiles and exposes the public surface expected by the tests
//! so the RED phase of the TDD cycle can produce running-but-failing assertions.
//! The GREEN phase (next commit) will replace the stub bodies with the real
//! palette-driven projection.
//!
//! `render_ls_colors` / `render_eza_colors` / `render_strings` and the
//! `FILE_TYPE_KIND_KEYS` table are consumed by Plan 16-04 (`SharedShellModel`)
//! in Wave 2 — until that wave lands they are `dead_code` for non-test
//! compilation; the `allow(dead_code)` annotations below are intentional and
//! will be dropped once Plan 16-04 wires the call sites.

#![allow(dead_code)]

use crate::adapter::{ApplyOutcome, ApplyStrategy, ToolAdapter};
use crate::cli::picker::preview_panel::SemanticColor;
use crate::error::Result;
use crate::theme::{Palette, ThemeVariant};
use std::path::PathBuf;

static FILE_TYPE_KIND_KEYS: &[(&str, SemanticColor)] = &[
    ("di", SemanticColor::FileDir),
    ("ln", SemanticColor::FileSymlink),
    ("ex", SemanticColor::FileExec),
    ("or", SemanticColor::FileSymlink),
    ("so", SemanticColor::FileExec),
    ("pi", SemanticColor::FileConfig),
    ("bd", SemanticColor::FileConfig),
    ("cd", SemanticColor::FileConfig),
];

fn ansi_code(_palette: &Palette, _role: SemanticColor) -> String {
    // RED stub — will be implemented in GREEN.
    String::new()
}

pub(crate) fn render_ls_colors(_palette: &Palette) -> String {
    // RED stub — returns empty string so tests fail meaningfully.
    String::new()
}

pub(crate) fn render_eza_colors(_palette: &Palette) -> String {
    // RED stub — returns empty string so tests fail meaningfully.
    String::new()
}

pub(crate) fn render_strings(palette: &Palette) -> (String, String) {
    (render_ls_colors(palette), render_eza_colors(palette))
}

pub struct LsColorsAdapter;

impl ToolAdapter for LsColorsAdapter {
    fn tool_name(&self) -> &'static str {
        "ls_colors"
    }

    fn is_installed(&self) -> Result<bool> {
        // RED stub — intentionally wrong so the adapter test fails.
        Ok(false)
    }

    fn integration_config_path(&self) -> Result<PathBuf> {
        Ok(PathBuf::new())
    }

    fn managed_config_path(&self) -> PathBuf {
        PathBuf::new()
    }

    fn apply_strategy(&self) -> ApplyStrategy {
        // RED stub — intentionally wrong strategy so the test fails.
        ApplyStrategy::SourceScript
    }

    fn apply_theme(&self, _theme: &ThemeVariant) -> Result<ApplyOutcome> {
        // RED stub — wrong signal so the test fails.
        Ok(ApplyOutcome::Applied {
            requires_new_shell: false,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::design::file_type_colors::{extension_map, full_name_map};
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

        let cases: &[(&str, FileKind, &str)] = &[
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
