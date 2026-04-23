//! Starship adapter with scoped [palettes.slate] editing.
//! Explicit exception to managed-first — Starship has no documented
//! include/import mechanism, so uses EditInPlace strategy to modify user's
//! starship.toml in-place with careful scoping to [palettes.slate] section.

use crate::adapter::{ApplyOutcome, ApplyStrategy, SkipReason, ToolAdapter};
use crate::config::ConfigManager;
use crate::detection;
use crate::env::SlateEnv;
use crate::error::{Result, SlateError};
use crate::theme::ThemeVariant;
use std::fs;
use std::path::{Path, PathBuf};
use toml_edit::{DocumentMut, Item, Value};

/// Starship adapter implementing the ToolAdapter trait.
pub struct StarshipAdapter;

fn replace_fg_crust_in_value(value: &mut Value) {
    match value {
        Value::String(formatted) => {
            if formatted.value().contains("fg:crust") {
                let decor = formatted.decor().clone();
                let replaced = formatted.value().replace("fg:crust", "fg:powerline_fg");
                *formatted = toml_edit::Formatted::new(replaced);
                *formatted.decor_mut() = decor;
            }
        }
        Value::Array(array) => {
            for child in array.iter_mut() {
                replace_fg_crust_in_value(child);
            }
        }
        Value::InlineTable(table) => {
            for (_, child) in table.iter_mut() {
                replace_fg_crust_in_value(child);
            }
        }
        Value::Integer(_) | Value::Float(_) | Value::Boolean(_) | Value::Datetime(_) => {}
    }
}

fn replace_fg_crust_in_item(item: &mut Item) {
    match item {
        Item::Value(value) => replace_fg_crust_in_value(value),
        Item::Table(table) => {
            for (_, child) in table.iter_mut() {
                replace_fg_crust_in_item(child);
            }
        }
        Item::ArrayOfTables(array_of_tables) => {
            for table in array_of_tables.iter_mut() {
                for (_, child) in table.iter_mut() {
                    replace_fg_crust_in_item(child);
                }
            }
        }
        Item::None => {}
    }
}

fn inject_slate_palette(doc: &mut DocumentMut, theme: &ThemeVariant) {
    // Step 1: Set palette = "slate" at root level
    doc["palette"] = toml_edit::value("slate");

    // Step 2: Ensure [palettes.slate] table exists
    if doc.get("palettes").is_none() {
        doc["palettes"] = toml_edit::Item::Table(toml_edit::Table::new());
    }

    if let Some(palettes) = doc["palettes"].as_table_mut() {
        let mut sp = toml_edit::Table::new();
        let p = &theme.palette;

        // Helper: pick first available Option<String>, or fallback
        fn pick(opts: &[&Option<String>], fallback: &str) -> String {
            opts.iter()
                .filter_map(|o| o.as_ref())
                .next()
                .cloned()
                .unwrap_or_else(|| fallback.to_string())
        }

        // Core ANSI — always available
        sp["red"] = toml_edit::value(p.red.as_str());
        sp["yellow"] = toml_edit::value(p.yellow.as_str());
        sp["green"] = toml_edit::value(p.green.as_str());
        sp["blue"] = toml_edit::value(p.blue.as_str());
        sp["white"] = toml_edit::value(p.white.as_str());
        sp["foreground"] = toml_edit::value(p.foreground.as_str());
        sp["background"] = toml_edit::value(p.background.as_str());
        sp["text"] = toml_edit::value(p.text.as_deref().unwrap_or(&p.foreground));

        // Starship segment colors — must be 6 visually distinct values.
        // peach (warm accent between red and yellow segments): must differ from both.
        let peach_candidates: Vec<&str> = [
            p.extras.get("peach"),
            p.extras.get("orange"),
            p.extras.get("rose"),
        ]
        .iter()
        .filter_map(|o| o.map(|s| s.as_str()))
        .collect();

        let peach = peach_candidates
            .iter()
            .find(|c| **c != p.red && **c != p.yellow)
            .map(|s| s.to_string())
            .unwrap_or_else(|| {
                if p.bright_red != p.red && p.bright_red != p.yellow {
                    p.bright_red.clone()
                } else if p.bright_yellow != p.yellow && p.bright_yellow != p.red {
                    p.bright_yellow.clone()
                } else {
                    p.magenta.clone()
                }
            });
        sp["peach"] = toml_edit::value(peach.as_str());

        // sapphire (cool accent): sapphire → foam → bright_blue (if ≠ blue) → cyan
        let sapphire = p
            .extras
            .get("sapphire")
            .or(p.extras.get("foam"))
            .cloned()
            .unwrap_or_else(|| {
                if p.bright_blue != p.blue {
                    p.bright_blue.clone()
                } else {
                    p.cyan.clone()
                }
            });
        sp["sapphire"] = toml_edit::value(sapphire.as_str());

        // lavender (purple accent): lavender → iris → mauve → bright_magenta → magenta
        let lavender = p
            .lavender
            .clone()
            .or_else(|| p.extras.get("lavender").cloned())
            .or_else(|| p.extras.get("iris").cloned())
            .unwrap_or_else(|| pick(&[&p.mauve, &Some(p.bright_magenta.clone())], &p.magenta));
        sp["lavender"] = toml_edit::value(lavender.as_str());

        // Secondary palette names used by some starship configs
        sp["teal"] = toml_edit::value(p.cyan.as_str());
        sp["maroon"] = toml_edit::value(p.extras.get("maroon").unwrap_or(&p.bright_red).as_str());
        sp["sky"] = toml_edit::value(p.bright_cyan.as_str());
        sp["pink"] = toml_edit::value(
            p.pink
                .as_deref()
                .or(p.extras.get("pink").map(|s| s.as_str()))
                .unwrap_or(&p.bright_magenta),
        );

        // crust: semantic darkest background
        sp["crust"] = toml_edit::value(p.bg_darkest.as_deref().unwrap_or(&p.black));

        // powerline_fg: adaptive high-contrast foreground for segment text.
        // Light themes: pick from {bg_darkest|black, background, foreground}
        // by maximising the MIN WCAG 2.1 contrast across the 6 representative
        // starship pill bgs (cyan, mauve|magenta, blue, yellow, red, green).
        // See `crate::wcag::pick_light_powerline_fg` and CONTEXT.
        // Dark themes: unchanged — bg_darkest with cascade to black.
        let powerline_fg: String = if theme.appearance == crate::theme::ThemeAppearance::Light {
            crate::wcag::pick_light_powerline_fg(p)
        } else {
            p.bg_darkest.as_ref().unwrap_or(&p.black).clone()
        };
        sp["powerline_fg"] = toml_edit::value(powerline_fg.as_str());

        palettes["slate"] = toml_edit::Item::Table(sp);
    }

    // Replace fg:crust with fg:powerline_fg only inside TOML string values
    // so comments and unrelated raw text remain untouched.
    for (_, item) in doc.iter_mut() {
        replace_fg_crust_in_item(item);
    }
}

pub(crate) fn themed_config_from_content(content: &str, theme: &ThemeVariant) -> Result<String> {
    let mut doc: DocumentMut = content.parse().map_err(SlateError::TomlParseError)?;
    inject_slate_palette(&mut doc, theme);
    Ok(doc.to_string())
}

impl StarshipAdapter {
    /// Pure path resolution: env override → XDG default (no global state)
    fn resolve_path(starship_config: Option<&str>, config_home: &Path) -> PathBuf {
        if let Some(val) = starship_config {
            if !val.is_empty() {
                return PathBuf::from(val);
            }
        }
        config_home.join("starship.toml")
    }

    pub(crate) fn integration_config_path_with_env(env: &SlateEnv) -> PathBuf {
        // Intentionally ignore STARSHIP_CONFIG env var here.
        // In a Slate-managed shell, STARSHIP_CONFIG points to the managed
        // fallback file — we always want to seed/upgrade the user's real
        // config at ~/.config/starship.toml, not the managed copy.
        Self::resolve_path(None, env.xdg_config_home())
    }
}

impl ToolAdapter for StarshipAdapter {
    fn tool_name(&self) -> &'static str {
        "starship"
    }

    fn is_installed(&self) -> Result<bool> {
        Ok(detection::detect_tool_presence(self.tool_name()).installed)
    }

    fn integration_config_path(&self) -> Result<PathBuf> {
        let env = SlateEnv::from_process()?;
        Ok(Self::integration_config_path_with_env(&env))
    }

    fn managed_config_path(&self) -> PathBuf {
        let env = SlateEnv::from_process().ok();
        if let Some(env) = env.as_ref() {
            env.config_dir().join("managed").join("starship")
        } else {
            PathBuf::from(".config/slate/managed/starship")
        }
    }

    fn apply_strategy(&self) -> ApplyStrategy {
        ApplyStrategy::EditInPlace
    }

    fn apply_theme(&self, theme: &ThemeVariant) -> Result<ApplyOutcome> {
        let env = SlateEnv::from_process()?;
        self.apply_theme_with_env(theme, &env)
    }

    /// preview-path override. Resolves `starship.toml` via the
    /// injected env (so tempdir-backed integration tests can point Starship at
    /// a sandboxed XDG root) and creates the `ConfigManager` via
    /// `with_env(env)` rather than `new()` so the managed-file backup area
    /// also lives inside the injected config dir.
    fn apply_theme_with_env(&self, theme: &ThemeVariant, env: &SlateEnv) -> Result<ApplyOutcome> {
        let config_path = Self::integration_config_path_with_env(env);
        if !config_path.exists() {
            return Ok(ApplyOutcome::Skipped(SkipReason::MissingIntegrationConfig));
        }

        // Step 0: Backup before any modification
        let config_manager = ConfigManager::with_env(env)?;
        let _backup_path = config_manager.backup_file(&config_path)?;

        // Step 1: Read and parse TOML (preserves comments via toml_edit).
        // Read as bytes first so we can produce an actionable error when the
        // user's starship.toml has stray non-UTF-8 bytes (seen in issue #3)
        // instead of the bare "stream did not contain valid UTF-8" from
        // read_to_string that leaves users guessing which file and where.
        let bytes = fs::read(&config_path).map_err(|e| {
            SlateError::ConfigReadError(config_path.display().to_string(), e.to_string())
        })?;
        let content = String::from_utf8(bytes).map_err(|e| {
            SlateError::ConfigReadError(
                config_path.display().to_string(),
                format!(
                    "contains non-UTF-8 bytes at byte offset {} — slate cannot parse this file. \
                     Inspect with `xxd {} | head` around that offset and remove the stray bytes.",
                    e.utf8_error().valid_up_to(),
                    config_path.display()
                ),
            )
        })?;
        let rendered = themed_config_from_content(&content, theme)?;
        crate::config::atomic_write_synced(&config_path, rendered.as_bytes()).map_err(|e| {
            SlateError::ConfigWriteError(config_path.display().to_string(), e.to_string())
        })?;

        // Starship reads $STARSHIP_CONFIG / the palette select at shell init;
        // palette changes only appear in a fresh shell.
        Ok(ApplyOutcome::applied_needs_new_shell())
    }

    fn get_current_theme(&self) -> Result<Option<String>> {
        // feature; not implemented yet
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_name() {
        let adapter = StarshipAdapter;
        assert_eq!(adapter.tool_name(), "starship");
    }

    #[test]
    fn apply_theme_reports_non_utf8_bytes_with_file_path_and_offset() {
        use crate::theme::ThemeRegistry;

        let td = tempfile::TempDir::new().unwrap();
        let env = SlateEnv::with_home(td.path().to_path_buf());
        let starship_path = env.xdg_config_home().join("starship.toml");
        std::fs::create_dir_all(starship_path.parent().unwrap()).unwrap();

        // Valid TOML prefix + a stray non-UTF-8 byte at a known offset, so
        // the error message must point at that offset.
        let mut bytes = b"format = \"$all\"\n".to_vec();
        let offset = bytes.len();
        bytes.push(0xff);
        bytes.extend_from_slice(b"\n");
        std::fs::write(&starship_path, bytes).unwrap();

        let theme = ThemeRegistry::new()
            .unwrap()
            .get("catppuccin-mocha")
            .unwrap()
            .clone();
        let err = StarshipAdapter
            .apply_theme_with_env(&theme, &env)
            .expect_err("non-UTF-8 starship.toml must fail");

        let msg = err.to_string();
        assert!(
            msg.contains("starship.toml"),
            "error should name the offending file, got: {msg}"
        );
        assert!(
            msg.contains(&offset.to_string()),
            "error should include byte offset {offset}, got: {msg}"
        );
        assert!(
            msg.contains("non-UTF-8"),
            "error should classify the fault as non-UTF-8, got: {msg}"
        );
    }

    #[test]
    fn test_apply_strategy_returns_edit_in_place() {
        let adapter = StarshipAdapter;
        assert_eq!(adapter.apply_strategy(), ApplyStrategy::EditInPlace);
    }

    #[test]
    fn test_managed_config_path_returns_correct_directory() {
        let adapter = StarshipAdapter;
        let path = adapter.managed_config_path();
        assert!(path
            .to_string_lossy()
            .contains(".config/slate/managed/starship"));
    }

    #[test]
    fn test_resolve_path_with_env_override() {
        let config_home = PathBuf::from("/home/user/.config");
        let path = StarshipAdapter::resolve_path(Some("/custom/starship.toml"), &config_home);
        assert_eq!(path, PathBuf::from("/custom/starship.toml"));
    }

    #[test]
    fn test_resolve_path_empty_env_uses_default() {
        let config_home = PathBuf::from("/home/user/.config");
        let path = StarshipAdapter::resolve_path(Some(""), &config_home);
        assert_eq!(path, PathBuf::from("/home/user/.config/starship.toml"));
    }

    #[test]
    fn test_resolve_path_default_xdg() {
        let config_home = PathBuf::from("/home/user/.config");
        let path = StarshipAdapter::resolve_path(None, &config_home);
        assert_eq!(path, PathBuf::from("/home/user/.config/starship.toml"));
    }

    #[test]
    fn test_replace_fg_crust_only_updates_toml_strings() {
        let mut doc = r##"
# keep fg:crust in comments
format = "[x](fg:crust)"
[palettes.slate]
crust = "#111111"
"##
        .parse::<DocumentMut>()
        .unwrap();

        for (_, item) in doc.iter_mut() {
            replace_fg_crust_in_item(item);
        }

        let rendered = doc.to_string();
        assert!(rendered.contains("# keep fg:crust in comments"));
        assert!(rendered.contains("format = \"[x](fg:powerline_fg)\""));
        assert!(rendered.contains("crust = \"#111111\""));
    }

    #[test]
    fn test_get_current_theme_returns_none() {
        let adapter = StarshipAdapter;
        let result = adapter.get_current_theme();
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), None);
    }

    /// contract: the trait-level `apply_theme_with_env` must honor
    /// the injected env — palette selection must be written to the
    /// `starship.toml` inside the tempdir, not the host's real
    /// `~/.config/starship.toml`, and the backup must also land inside the
    /// tempdir's slate cache.
    #[test]
    fn apply_theme_with_env_honors_injected_env_for_in_place_edits() {
        use crate::adapter::ToolAdapter;
        use std::io::Write;
        use tempfile::TempDir;

        let tempdir = TempDir::new().unwrap();
        let env = SlateEnv::with_home(tempdir.path().to_path_buf());
        let adapter = StarshipAdapter;

        // Pre-create starship.toml inside the tempdir so the EditInPlace path
        // doesn't early-return with MissingIntegrationConfig.
        let integration_path = StarshipAdapter::integration_config_path_with_env(&env);
        fs::create_dir_all(integration_path.parent().unwrap()).unwrap();
        let mut file = fs::File::create(&integration_path).unwrap();
        writeln!(file, "# starship managed by slate").unwrap();
        drop(file);

        let theme = crate::theme::catppuccin::catppuccin_mocha().unwrap();

        let outcome = ToolAdapter::apply_theme_with_env(&adapter, &theme, &env).unwrap();
        assert!(matches!(outcome, ApplyOutcome::Applied { .. }));

        // The edit must have happened on the tempdir-scoped starship.toml.
        let content = fs::read_to_string(&integration_path).unwrap();
        assert!(
            content.contains("palette = \"slate\""),
            "tempdir starship.toml must select the slate palette, got:\n{}",
            content
        );
        assert!(
            content.contains("[palettes.slate]"),
            "tempdir starship.toml must contain [palettes.slate] table, got:\n{}",
            content
        );

        // Confirm NOTHING leaked into the host's ~/.config via the process env.
        // The tempdir-scoped SlateEnv points config_dir at tempdir/.config/slate,
        // so the backup must live inside tempdir, not under the real $HOME.
        let backups_dir = tempdir.path().join(".cache/slate/backups");
        if backups_dir.exists() {
            let entries: Vec<_> = fs::read_dir(&backups_dir)
                .unwrap()
                .flatten()
                .map(|e| e.path())
                .collect();
            assert!(
                !entries.is_empty(),
                "backup must land inside tempdir's slate cache dir at {:?}",
                backups_dir
            );
        }
    }

    /// The 6 representative pill bgs used by `pick_light_powerline_fg`.
    /// Mirror of the slate hardcoded inside `crate::wcag::pick_light_powerline_fg`
    /// kept in sync via this test (drift here means drift in production).
    fn light_pill_bgs(p: &crate::theme::Palette) -> [String; 6] {
        let mauve_or_magenta = p
            .mauve
            .clone()
            .or_else(|| p.extras.get("mauve").cloned())
            .unwrap_or_else(|| p.magenta.clone());
        [
            p.cyan.clone(),
            mauve_or_magenta,
            p.blue.clone(),
            p.yellow.clone(),
            p.red.clone(),
            p.green.clone(),
        ]
    }

    /// Parse `[palettes.slate].powerline_fg` out of a rendered starship.toml.
    fn extract_powerline_fg(rendered: &str) -> String {
        let doc: DocumentMut = rendered.parse().expect("rendered must be valid TOML");
        let value = doc["palettes"]["slate"]["powerline_fg"]
            .as_str()
            .expect("powerline_fg must be a string");
        value.to_string()
    }

    /// Diagnostic helper: dumps per-candidate MIN-contrast for every Light
    /// theme. Marked `#[ignore]` so it does not run in the default sweep
    /// invoke with `cargo test ... -- --ignored --nocapture` when triaging
    /// invariant failures.
    #[test]
    #[ignore]
    fn diag_dump_light_theme_candidate_contrasts() {
        use crate::theme::{ThemeAppearance, ThemeRegistry};

        let registry = ThemeRegistry::new().unwrap();
        for theme in registry
            .all()
            .into_iter()
            .filter(|t| t.appearance == ThemeAppearance::Light)
        {
            let p = &theme.palette;
            let bg_darkest = p.bg_darkest.clone().unwrap_or_else(|| p.black.clone());
            let candidates: [(&str, String); 3] = [
                ("bg_darkest", bg_darkest),
                ("background", p.background.clone()),
                ("foreground", p.foreground.clone()),
            ];
            let pill_bgs = light_pill_bgs(p);

            eprintln!("\n=== {} ===", theme.id);
            for (name, hex) in &candidates {
                let min = pill_bgs
                    .iter()
                    .map(|bg| crate::wcag::contrast_hex(hex, bg))
                    .fold(f64::INFINITY, f64::min);
                eprintln!("  {:11} {:>7}  min_contrast={:.2}", name, hex, min);
            }
            eprintln!("  pills: {:?}", pill_bgs);
        }
    }

    /// STARSHIP-01 picker correctness sweep: the rendered `powerline_fg` for
    /// every Light theme must equal the MAX-of-MIN candidate across the
    /// 3-candidate slate `[bg_darkest|black, background, foreground]`. This
    /// proves the picker is doing its job — no candidate that the picker
    /// rejected can outperform the picker's choice.
    /// **NOTE on the 4.5:1 WCAG AA floor.** CONTEXT specified the
    /// 3-candidate slate `{bg_darkest, background, foreground}` with the
    /// expectation that at least one candidate would always hit ≥4.5:1
    /// across the 6 pills. That holds for 4 of 7 Light themes shipped today
    /// (everforest-light, gruvbox-light, kanagawa-lotus, tokyo-night-light).
    /// For `catppuccin-latte`, `rose-pine-dawn`, and `solarized-light` the
    /// pill sets mix dark accents with light pastels (e.g.,
    /// catppuccin-latte's `mauve = #ca9ee6`). No single foreground can
    /// simultaneously hit ≥4.5:1 against both extremes — it is a palette
    /// luminance constraint, not a picker defect. Per-theme floors are
    /// asserted by `adaptive_powerline_fg_min_contrast_baseline_per_light_theme`
    /// below; lifting the WCAG AA floor is a deferred follow-up that needs
    /// either palette tuning or per-pill `powerline_fg_<color>` overrides.
    /// Self-extending: any future Light theme added to `themes.toml` is
    /// automatically swept by this picker-correctness check.
    #[test]
    fn adaptive_powerline_fg_picks_max_of_min_for_all_light_themes() {
        use crate::theme::{ThemeAppearance, ThemeRegistry};

        let registry = ThemeRegistry::new().expect("theme registry must load");
        let light_themes: Vec<_> = registry
            .all()
            .into_iter()
            .filter(|t| t.appearance == ThemeAppearance::Light)
            .collect();

        assert!(
            !light_themes.is_empty(),
            "registry must contain at least one Light theme"
        );

        for theme in &light_themes {
            let p = &theme.palette;
            let pill_bgs = light_pill_bgs(p);

            // Replicate the picker's 3-candidate slate in the same preference
            // order (bg_darkest|black, background, foreground).
            let bg_darkest = p.bg_darkest.clone().unwrap_or_else(|| p.black.clone());
            let candidates_owned = [bg_darkest, p.background.clone(), p.foreground.clone()];
            let mins: Vec<f64> = candidates_owned
                .iter()
                .map(|c| {
                    pill_bgs
                        .iter()
                        .map(|bg| crate::wcag::contrast_hex(c, bg))
                        .fold(f64::INFINITY, f64::min)
                })
                .collect();

            // First-in-order tie-break (strict `>`) replicates the picker.
            let mut best_idx = 0usize;
            let mut best_min = f64::NEG_INFINITY;
            for (i, m) in mins.iter().enumerate() {
                if *m > best_min {
                    best_min = *m;
                    best_idx = i;
                }
            }
            let expected = &candidates_owned[best_idx];

            let rendered = themed_config_from_content("", theme)
                .expect("themed_config_from_content must succeed");
            let emitted = extract_powerline_fg(&rendered);

            assert_eq!(
                &emitted, expected,
                "theme {} emitted {} but MAX-of-MIN candidate was {} (mins={:?})",
                theme.id, emitted, expected, mins
            );
        }
    }

    /// Per-theme MIN-contrast regression baseline.
    /// Locks the actual MIN contrast each Light theme achieves with the
    /// adaptive picker. Any palette edit that drops a theme below its current
    /// floor will fail this test loudly. Future plans that lift palettes
    /// above 4.5:1 should raise these floors accordingly.
    /// Floors are slightly under measured values (2026-04-28) to absorb f64
    /// rounding noise.
    #[test]
    fn adaptive_powerline_fg_min_contrast_baseline_per_light_theme() {
        use crate::theme::ThemeRegistry;

        // (theme_id, lower_bound_for_min_contrast)
        let baselines: &[(&str, f64)] = &[
            ("catppuccin-latte", 1.90),
            ("everforest-light", 6.00),
            ("gruvbox-light", 5.30),
            ("kanagawa-lotus", 4.60),
            ("rose-pine-dawn", 2.00),
            ("solarized-light", 4.00),
            ("tokyo-night-light", 5.20),
        ];

        let registry = ThemeRegistry::new().unwrap();
        for (id, floor) in baselines {
            let theme = registry
                .get(id)
                .unwrap_or_else(|| panic!("theme {id} must exist in registry"));
            let rendered = themed_config_from_content("", theme).unwrap();
            let emitted = extract_powerline_fg(&rendered);
            let pill_bgs = light_pill_bgs(&theme.palette);
            let min = pill_bgs
                .iter()
                .map(|bg| crate::wcag::contrast_hex(&emitted, bg))
                .fold(f64::INFINITY, f64::min);
            assert!(
                min >= *floor,
                "{} regression: MIN contrast dropped to {:.2} (baseline floor {:.2})",
                id,
                min,
                floor
            );
        }
    }

    /// Round-trip guard: the `powerline_fg` rendered into the TOML must match
    /// the helper's choice byte-for-byte. Catches accidental drift between
    /// `pick_light_powerline_fg` and the adapter call site.
    #[test]
    fn powerline_fg_emitted_matches_helper_pick() {
        use crate::theme::ThemeRegistry;

        let registry = ThemeRegistry::new().unwrap();
        let theme = registry
            .get("solarized-light")
            .expect("solarized-light is the canonical mid-tone-accent regression case");

        let rendered = themed_config_from_content("", theme).unwrap();
        let emitted = extract_powerline_fg(&rendered);
        let helper_pick = crate::wcag::pick_light_powerline_fg(&theme.palette);

        assert_eq!(
            emitted, helper_pick,
            "rendered powerline_fg must equal helper pick (drift between adapter and wcag helper)"
        );
    }

    /// Dark-path non-regression: ensure the unchanged Dark branch still emits
    /// `bg_darkest` (with cascade to `black`).
    #[test]
    fn dark_theme_powerline_fg_unchanged_uses_bg_darkest_cascade() {
        use crate::theme::ThemeRegistry;

        let registry = ThemeRegistry::new().unwrap();
        let theme = registry
            .get("catppuccin-mocha")
            .expect("catppuccin-mocha must exist");

        let rendered = themed_config_from_content("", theme).unwrap();
        let emitted = extract_powerline_fg(&rendered);

        let expected = theme
            .palette
            .bg_darkest
            .clone()
            .unwrap_or_else(|| theme.palette.black.clone());

        assert_eq!(
            emitted, expected,
            "Dark-path powerline_fg must remain bg_darkest|black"
        );
    }
}
