use crate::adapter::font::{FontAdapter, FontDiscovery};
use crate::adapter::{AlacrittyAdapter, GhosttyAdapter, KittyAdapter};
use crate::brand::events::{dispatch, BrandEvent, FailureKind, SuccessKind};
use crate::brand::render_context::RenderContext;
use crate::brand::roles::Roles;
use crate::cli::font_selection::FontCatalog;
use crate::env::SlateEnv;
use crate::error::{Result, SlateError};
use std::fs;
use std::path::{Path, PathBuf};

fn font_uses_basic_prompt(font_name: &str) -> bool {
    !FontAdapter::is_nerd_font_name(font_name)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ResolvedFontChoice {
    Installed(String),
    Catalog(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FontApplyReport {
    applied: Vec<&'static str>,
    skipped: Vec<(&'static str, &'static str)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TerminalRefSyntax {
    Ghostty,
    Alacritty,
    Kitty,
}

impl ResolvedFontChoice {
    pub(crate) fn font_name(&self) -> &str {
        match self {
            Self::Installed(name) | Self::Catalog(name) => name,
        }
    }
}

fn find_installed_font(discovery: &FontDiscovery, requested_key: &str) -> Option<String> {
    discovery
        .nerd_fonts
        .iter()
        .chain(discovery.system_fonts.iter())
        .find(|font| FontAdapter::family_match_key(font) == requested_key)
        .cloned()
}

fn resolve_font_choice_with_discovery(
    name: &str,
    discovery: &FontDiscovery,
) -> Result<ResolvedFontChoice> {
    let requested_key = FontAdapter::family_match_key(name);

    if let Some(installed) = find_installed_font(discovery, &requested_key) {
        return Ok(ResolvedFontChoice::Installed(installed));
    }

    if let Some(catalog_font) = FontCatalog::all_fonts().into_iter().find(|font| {
        font.name == name
            || font.id == name
            || FontAdapter::family_match_key(font.name) == requested_key
            || FontAdapter::family_match_key(font.id) == requested_key
    }) {
        let canonical_key = FontAdapter::family_match_key(catalog_font.name);
        if let Some(installed) = find_installed_font(discovery, &canonical_key) {
            return Ok(ResolvedFontChoice::Installed(installed));
        }

        return Ok(ResolvedFontChoice::Catalog(catalog_font.name.to_string()));
    }

    Err(SlateError::InvalidConfig(format!(
        "Font '{}' not found. Run 'slate font' to see available options.",
        name
    )))
}

pub(crate) fn resolve_font_choice(name: &str) -> Result<ResolvedFontChoice> {
    let discovery = FontAdapter::discover_all_fonts()?;
    resolve_font_choice_with_discovery(name, &discovery)
}

fn file_contains_managed_ref(path: &Path, managed_path: &Path, syntax: TerminalRefSyntax) -> bool {
    match syntax {
        TerminalRefSyntax::Ghostty => {
            text_file_contains_directive_path(path, managed_path, &[b"config-file", b"include"])
        }
        TerminalRefSyntax::Alacritty => alacritty_file_contains_import(path, managed_path),
        TerminalRefSyntax::Kitty => {
            text_file_contains_directive_path(path, managed_path, &[b"include"])
        }
    }
}

fn text_file_contains_directive_path(path: &Path, managed_path: &Path, keys: &[&[u8]]) -> bool {
    let managed = managed_path.display().to_string();
    let Ok(content) = fs::read(path) else {
        return false;
    };
    let managed = managed.as_bytes();

    content
        .split(|b| *b == b'\n')
        .any(|line| line_contains_directive_path(line, managed, keys))
}

fn alacritty_file_contains_import(path: &Path, managed_path: &Path) -> bool {
    let managed = managed_path.display().to_string();
    let Ok(content) = fs::read_to_string(path) else {
        return false;
    };
    let Ok(doc) = content.parse::<toml_edit::DocumentMut>() else {
        return false;
    };

    toml_import_array_contains_managed(
        doc.get("general").and_then(|general| general.get("import")),
        &managed,
    ) || toml_import_array_contains_managed(doc.get("import"), &managed)
}

fn toml_import_array_contains_managed(item: Option<&toml_edit::Item>, managed_path: &str) -> bool {
    item.and_then(|item| item.as_array()).is_some_and(|array| {
        array
            .iter()
            .any(|value| value.as_str().is_some_and(|path| path == managed_path))
    })
}

fn line_contains_directive_path(line: &[u8], managed_path: &[u8], keys: &[&[u8]]) -> bool {
    let line = trim_ascii_space(line);
    if line.is_empty() || line.starts_with(b"#") {
        return false;
    }

    let key_end = line
        .iter()
        .position(|b| *b == b'=' || b.is_ascii_whitespace())
        .unwrap_or(line.len());
    let key = trim_ascii_space(&line[..key_end]);

    keys.contains(&key) && contains_path_reference(line, managed_path)
}

fn contains_path_reference(line: &[u8], managed_path: &[u8]) -> bool {
    if managed_path.is_empty() || line.len() < managed_path.len() {
        return false;
    }

    line.windows(managed_path.len())
        .enumerate()
        .any(|(idx, window)| {
            if window != managed_path {
                return false;
            }

            let previous = if idx == 0 {
                None
            } else {
                line.get(idx - 1).copied()
            };

            path_reference_starts_at_value_boundary(previous)
                && path_reference_ends_at_value_boundary(
                    line.get(idx + managed_path.len()).copied(),
                )
        })
}

fn path_reference_starts_at_value_boundary(previous: Option<u8>) -> bool {
    match previous {
        Some(b'=') | Some(b'"') | Some(b'\'') | Some(b'[') | Some(b'(') | Some(b'{')
        | Some(b',') => true,
        Some(prev) => prev.is_ascii_whitespace(),
        None => true,
    }
}

fn path_reference_ends_at_value_boundary(next: Option<u8>) -> bool {
    match next {
        Some(b'"') | Some(b'\'') | Some(b',') | Some(b']') | Some(b')') | Some(b'}')
        | Some(b'\r') | Some(b'\n') | None => true,
        Some(next) => next.is_ascii_whitespace(),
    }
}

fn trim_ascii_space(bytes: &[u8]) -> &[u8] {
    let start = bytes
        .iter()
        .position(|b| !b.is_ascii_whitespace())
        .unwrap_or(bytes.len());
    let end = bytes
        .iter()
        .rposition(|b| !b.is_ascii_whitespace())
        .map(|idx| idx + 1)
        .unwrap_or(start);
    &bytes[start..end]
}

fn target_status(
    name: &'static str,
    entry_paths: Vec<PathBuf>,
    managed_path: PathBuf,
    syntax: TerminalRefSyntax,
    missing_reason: &'static str,
    unlinked_reason: &'static str,
) -> (&'static str, Option<(&'static str, &'static str)>) {
    let existing = entry_paths
        .iter()
        .filter(|path| path.exists())
        .collect::<Vec<_>>();

    if existing
        .iter()
        .any(|path| file_contains_managed_ref(path, &managed_path, syntax))
    {
        return (name, None);
    }

    let reason = if existing.is_empty() {
        missing_reason
    } else {
        unlinked_reason
    };
    (name, Some((name, reason)))
}

fn collect_font_apply_report(env: &SlateEnv) -> FontApplyReport {
    let config_dir = env.config_dir();
    let ghostty_adapter = GhosttyAdapter;
    let ghostty_paths = ghostty_adapter
        .integration_candidate_paths_with_env(env)
        .unwrap_or_default();
    let alacritty_path = AlacrittyAdapter::integration_config_path_with_env(env);
    let kitty_path = KittyAdapter::resolve_config_path_with_env(env);

    let checks = [
        target_status(
            "Ghostty",
            ghostty_paths,
            config_dir.join("managed/ghostty/font.conf"),
            TerminalRefSyntax::Ghostty,
            "missing Ghostty config",
            "no Slate font include found",
        ),
        target_status(
            "Alacritty",
            vec![alacritty_path],
            config_dir.join("managed/alacritty/font.toml"),
            TerminalRefSyntax::Alacritty,
            "missing alacritty.toml",
            "no Slate font import found",
        ),
        target_status(
            "Kitty",
            vec![kitty_path],
            config_dir.join("managed/kitty/font.conf"),
            TerminalRefSyntax::Kitty,
            "missing kitty.conf",
            "no Slate font include found",
        ),
    ];

    let mut applied = Vec::new();
    let mut skipped = Vec::new();
    for (name, skip) in checks {
        if let Some(skip) = skip {
            skipped.push(skip);
        } else {
            applied.push(name);
        }
    }

    FontApplyReport { applied, skipped }
}

/// Handle `slate font` command
/// Supports two modes:
/// 1. `slate font <name>` — Apply explicit font directly
/// 2. `slate font` (no args) — Launch interactive font picker with Nerd + System groups
pub fn handle_font(font_name: Option<&str>) -> Result<()> {
    // Build a RenderContext up front so every status line in this
    // handler shares the same byte contract (sketch 003 canon +
    // daily chrome). graceful degrade: plain text when
    // the theme registry cannot boot.
    let ctx = RenderContext::from_active_theme().ok();
    let roles = ctx.as_ref().map(Roles::new);

    if let Some(name) = font_name {
        // Direct apply path: validate and apply font
        let env = SlateEnv::from_process()?;
        let selection = resolve_font_choice(name)?;
        let resolved_font = selection.font_name().to_string();

        if matches!(selection, ResolvedFontChoice::Catalog(_)) {
            eprintln!("Downloading {}...", resolved_font);
            match download_catalog_font(&resolved_font, &env) {
                Ok(()) => {
                    eprintln!("{}", format_font_downloaded(roles.as_ref(), &resolved_font));
                    // font-download success → FontDownloaded event
                    // (SoundSink maps this to the font-install SFX).
                    dispatch(BrandEvent::Success(SuccessKind::FontDownloaded));
                }
                Err(err) => {
                    dispatch(BrandEvent::Failure(FailureKind::FontDownloadFailed));
                    return Err(SlateError::InvalidConfig(format!(
                        "Font '{}' could not be installed: {}",
                        resolved_font, err
                    )));
                }
            }
        }

        FontAdapter::apply_font(&env, &resolved_font)?;

        println!("{}", format_font_updated(roles.as_ref(), &resolved_font));
        if let Some(report) =
            format_font_apply_report(roles.as_ref(), &collect_font_apply_report(&env))
        {
            println!("{report}");
        }
        // UX-02 (D-D2 + D-D3): the font adapter is always RequiresNewShell=true
        // per D-C3, and this handler bypasses `apply_all`, so we emit inline.
        // Positioned BEFORE the font-specific `activation_hint` line so the
        // two coexist in the correct order (reveal first, activation-hint
        // second). `slate font` has no --auto / --quiet flags — both guards
        // are false.
        crate::cli::new_shell_reminder::emit_new_shell_reminder_once(false, false);
        if font_uses_basic_prompt(&resolved_font) {
            println!("(i) Basic Starship mode enabled for new shells because this font does not include Nerd Font glyphs.");
        } else {
            println!("{}", crate::platform::fonts::activation_hint());
        }
        // whole-flow milestone so can latch onto a single
        // per-command completion moment independent of the per-step
        // FontDownloaded event.
        dispatch(BrandEvent::ApplyComplete);
        Ok(())
    } else {
        // Picker path: show font picker UI
        show_font_picker(roles.as_ref())
    }
}

/// Format `✓ <font> downloaded` — success line emitted after a catalog
/// download completes. Routes through `Roles::status_success` so the ✓
/// glyph carries theme.green (never lavender per D-01a).
fn format_font_downloaded(r: Option<&Roles<'_>>, font_name: &str) -> String {
    match r {
        Some(r) => r.status_success(&format!("{} downloaded", font_name)),
        None => format!("✓ {} downloaded", font_name),
    }
}

/// Format `✓ Updated font to <font> in Slate-managed terminal configs.`
/// the main post-apply confirmation. Font name carried via
/// `Roles::path` to match the "file-system / config path" role (daily
/// chrome dim+italic, no theme-accent injection).
fn format_font_updated(r: Option<&Roles<'_>>, font_name: &str) -> String {
    match r {
        Some(r) => r.status_success(&format!(
            "Updated font to {} in Slate-managed terminal configs.",
            r.path(font_name)
        )),
        None => format!(
            "✓ Updated font to {} in Slate-managed terminal configs.",
            font_name
        ),
    }
}

fn format_font_apply_report(r: Option<&Roles<'_>>, report: &FontApplyReport) -> Option<String> {
    if report.applied.is_empty() && report.skipped.is_empty() {
        return None;
    }

    let applied = if report.applied.is_empty() {
        "none".to_string()
    } else {
        report.applied.join(", ")
    };
    let skipped = if report.skipped.is_empty() {
        "none".to_string()
    } else {
        report
            .skipped
            .iter()
            .map(|(tool, reason)| format!("{tool} ({reason})"))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let line = format!("Terminal font refs: applied to {applied}; skipped {skipped}.");

    Some(match r {
        Some(r) => r.path(&line),
        None => format!("(i) {line}"),
    })
}

/// Format `✗ Download failed: <reason>` via `Roles::status_error`
/// (theme.red — NEVER lavender per D-01a).
fn format_font_download_failed(r: Option<&Roles<'_>>, reason: &str) -> String {
    match r {
        Some(r) => r.status_error(&format!("Download failed: {}", reason)),
        None => format!("✗ Download failed: {}", reason),
    }
}

/// Format the picker's "no supported fonts" fallback error.
fn format_no_fonts_found(r: Option<&Roles<'_>>) -> String {
    let body = "No supported fonts found. Run 'slate setup' to install the recommended Nerd Fonts.";
    match r {
        Some(r) => r.status_error(body),
        None => format!("✗ {body}"),
    }
}

/// Show interactive font picker with installed fonts + catalog fonts available for download.
fn show_font_picker(roles: Option<&Roles<'_>>) -> Result<()> {
    let env = SlateEnv::from_process()?;
    let discovery = FontAdapter::discover_all_fonts()?;

    // Build the set of installed nerd font family keys for quick lookup
    let installed_keys: std::collections::HashSet<String> = discovery
        .nerd_fonts
        .iter()
        .map(|f| FontAdapter::family_match_key(f))
        .collect();

    // Build picker items: (key, display_label, font_name, is_system, needs_install, hint)
    let mut picker_items: Vec<(String, String, String, bool, bool, &str)> = Vec::new();

    // ── Group 1: Nerd Fonts (installed) ──
    let mut is_first_nerd = true;
    if !discovery.nerd_fonts.is_empty() {
        // JetBrainsMono first + recommended marker
        for font in discovery.nerd_fonts.iter() {
            if font.contains("JetBrainsMono") {
                let hint = if is_first_nerd { "Nerd Fonts" } else { "" };
                is_first_nerd = false;
                picker_items.push((
                    "nerd_jetbrains".to_string(),
                    format!("✦ {} (recommended)", font),
                    font.clone(),
                    false,
                    false,
                    hint,
                ));
                break;
            }
        }

        for (idx, font) in discovery.nerd_fonts.iter().enumerate() {
            if !font.contains("JetBrainsMono") {
                let hint = if is_first_nerd { "Nerd Fonts" } else { "" };
                is_first_nerd = false;
                picker_items.push((
                    format!("nerd_{}", idx),
                    font.clone(),
                    font.clone(),
                    false,
                    false,
                    hint,
                ));
            }
        }
    }

    // ── Group 2: Catalog fonts not yet installed (available for download) ──
    let catalog_fonts = FontCatalog::all_fonts();
    let mut has_downloadable = false;
    for (idx, cat_font) in catalog_fonts.iter().enumerate() {
        let cat_key = FontAdapter::family_match_key(cat_font.name);
        if !installed_keys.contains(&cat_key) {
            let hint = if !has_downloadable {
                has_downloadable = true;
                "Available to Download"
            } else {
                ""
            };
            picker_items.push((
                format!("catalog_{}", idx),
                format!("{} (not installed)", cat_font.name),
                cat_font.name.to_string(),
                false,
                true,
                hint,
            ));
        }
    }

    // ── Group 3: System Fonts ──
    if !discovery.system_fonts.is_empty() {
        let mut is_first_system = true;
        for (idx, font) in discovery.system_fonts.iter().enumerate() {
            let hint = if is_first_system {
                is_first_system = false;
                "System (no icons)"
            } else {
                ""
            };
            picker_items.push((
                format!("system_{}", idx),
                font.clone(),
                font.clone(),
                true,
                false,
                hint,
            ));
        }
    }

    if picker_items.is_empty() {
        eprintln!("{}", format_no_fonts_found(roles));
        return Ok(());
    }

    // Hint if no installed Nerd Fonts
    if discovery.nerd_fonts.is_empty() && !has_downloadable {
        println!("(i) Run 'slate setup' to install the recommended Nerd Fonts");
    }

    // Launch picker
    cliclack::intro("✦ Change Font")?;

    let mut menu_builder = cliclack::select("Select font:");
    for (key, display_label, _, _, _, hint) in &picker_items {
        menu_builder = menu_builder.item(key.as_str(), display_label.as_str(), *hint);
    }

    let selected = menu_builder.interact()?;

    // Find the selected item
    for (key, display_label, font_name, is_system, needs_install, _) in &picker_items {
        if key != selected {
            continue;
        }

        // Extract bare font name (remove markers)
        let bare_name = display_label
            .trim_start_matches("✦ ")
            .trim_end_matches(" (recommended)")
            .trim_end_matches(" (not installed)")
            .to_string();

        // Download if needed
        if *needs_install {
            let spinner = cliclack::spinner();
            spinner.start(format!("Downloading {}...", bare_name));

            match download_catalog_font(font_name, &env) {
                Ok(_) => {
                    spinner.stop(format_font_downloaded(roles, &bare_name));
                    // picker-path catalog-install success.
                    dispatch(BrandEvent::Success(SuccessKind::FontDownloaded));
                }
                Err(e) => {
                    spinner.error(format_font_download_failed(roles, &e));
                    // picker-path download failure.
                    dispatch(BrandEvent::Failure(FailureKind::FontDownloadFailed));
                    return Ok(());
                }
            }
        }

        // Show system fonts warning
        if *is_system {
            println!("(i) System fonts lack Nerd Font icons. Slate will switch new shells to the basic Starship profile.");
        }

        // Apply font
        FontAdapter::apply_font(&env, &bare_name)?;

        println!("{}", format_font_updated(roles, &bare_name));
        if let Some(report) = format_font_apply_report(roles, &collect_font_apply_report(&env)) {
            println!("{report}");
        }

        if font_uses_basic_prompt(&bare_name) {
            println!("(i) Basic Starship mode enabled for new shells because this font does not include Nerd Font glyphs.");
        } else {
            println!("{}", crate::platform::fonts::activation_hint());
        }
        // whole-flow milestone on picker-path apply success.
        dispatch(BrandEvent::ApplyComplete);
        break;
    }

    Ok(())
}

/// Download a catalog font using the same fallback chain as setup.
fn download_catalog_font(font_name: &str, env: &SlateEnv) -> std::result::Result<(), String> {
    use crate::cli::setup_executor::{
        copy_font_from_caskroom, download_font_release, install_font,
    };

    // Resolve display name ("Hack Nerd Font") to catalog ID ("hack")
    // because download functions look up by ID.
    let font_id = FontCatalog::all_fonts()
        .into_iter()
        .find(|f| f.name == font_name)
        .map(|f| f.id.to_string());
    let lookup = font_id.as_deref().unwrap_or(font_name);

    if matches!(
        crate::platform::packages::detect_backend(),
        crate::platform::packages::PackageManagerBackend::Homebrew
    ) {
        if install_font(lookup).is_ok() {
            return Ok(());
        }

        if copy_font_from_caskroom(lookup, env).is_ok() {
            return Ok(());
        }
    }

    // Download from Nerd Fonts releases
    download_font_release(lookup, env).map_err(|e| {
        let full = e.to_string();
        full.strip_prefix("Internal error: ")
            .unwrap_or(&full)
            .to_string()
    })
}

#[cfg(test)]
mod tests {
    use super::{
        collect_font_apply_report, file_contains_managed_ref, format_font_apply_report,
        format_font_download_failed, format_font_downloaded, format_font_updated,
        format_no_fonts_found, resolve_font_choice_with_discovery, FontApplyReport,
        ResolvedFontChoice, TerminalRefSyntax,
    };
    use crate::adapter::font::FontDiscovery;
    use crate::brand::render_context::{mock_context_with_mode, mock_theme, RenderMode};
    use crate::brand::roles::Roles;
    use crate::cli::new_shell_reminder::REMINDER_TEST_LOCK;
    use crate::env::SlateEnv;
    use tempfile::TempDir;

    /// Mirrors the explicit-name branch emit in `handle_font`: the font
    /// adapter is always RequiresNewShell=true per D-C3, so the inline
    /// emission is not gated on an aggregator — every successful apply
    /// reaches the emitter (which then respects its own auto/quiet guards).
    fn font_handler_emit() {
        crate::cli::new_shell_reminder::emit_new_shell_reminder_once(false, false);
    }

    #[test]
    fn font_handler_emits_reminder_on_success() {
        let _guard = REMINDER_TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        crate::cli::new_shell_reminder::reset_reminder_flag_for_tests();

        font_handler_emit();

        assert!(
            crate::cli::new_shell_reminder::reminder_flag_for_tests(),
            "font handler must transition the reminder flag after a successful apply"
        );
    }

    #[test]
    fn test_resolve_font_choice_matches_catalog_id_to_installed_font() {
        let discovery = FontDiscovery {
            nerd_fonts: vec!["JetBrains Mono Nerd Font".to_string()],
            system_fonts: vec![],
        };

        let choice = resolve_font_choice_with_discovery("jetbrains-mono", &discovery).unwrap();

        assert_eq!(
            choice,
            ResolvedFontChoice::Installed("JetBrains Mono Nerd Font".to_string())
        );
    }

    #[test]
    fn test_resolve_font_choice_rejects_unknown_font() {
        let discovery = FontDiscovery {
            nerd_fonts: vec![],
            system_fonts: vec!["Menlo".to_string()],
        };

        let err = resolve_font_choice_with_discovery("Definitely Not A Font", &discovery)
            .unwrap_err()
            .to_string();

        assert!(err.contains("Font 'Definitely Not A Font' not found"));
    }

    #[test]
    fn font_apply_report_marks_real_refs_and_missing_entry_files() {
        let td = TempDir::new().unwrap();
        let env = SlateEnv::with_home(td.path().to_path_buf());
        let ghostty_dir = env.xdg_config_home().join("ghostty");
        let alacritty_dir = env.xdg_config_home().join("alacritty");
        std::fs::create_dir_all(&ghostty_dir).unwrap();
        std::fs::create_dir_all(&alacritty_dir).unwrap();

        let ghostty_font = env.config_dir().join("managed/ghostty/font.conf");
        let alacritty_font = env.config_dir().join("managed/alacritty/font.toml");
        std::fs::create_dir_all(ghostty_font.parent().unwrap()).unwrap();
        std::fs::create_dir_all(alacritty_font.parent().unwrap()).unwrap();
        std::fs::write(&ghostty_font, "font-family = \"JetBrains Mono\"\n").unwrap();
        std::fs::write(
            &alacritty_font,
            "[font.normal]\nfamily = \"JetBrains Mono\"\n",
        )
        .unwrap();
        std::fs::write(
            ghostty_dir.join("config.ghostty"),
            format!("config-file = \"{}\"\n", ghostty_font.display()),
        )
        .unwrap();
        std::fs::write(alacritty_dir.join("alacritty.toml"), "[general]\n").unwrap();

        let report = collect_font_apply_report(&env);

        assert_eq!(report.applied, vec!["Ghostty"]);
        assert!(report
            .skipped
            .contains(&("Alacritty", "no Slate font import found")));
        assert!(report.skipped.contains(&("Kitty", "missing kitty.conf")));
    }

    #[test]
    fn font_apply_report_plain_formatter_lists_applied_and_skipped_targets() {
        let report = FontApplyReport {
            applied: vec!["Ghostty"],
            skipped: vec![("Kitty", "missing kitty.conf")],
        };

        let out = format_font_apply_report(None, &report).unwrap();

        assert_eq!(
            out,
            "(i) Terminal font refs: applied to Ghostty; skipped Kitty (missing kitty.conf)."
        );
    }

    #[test]
    fn font_apply_report_ignores_commented_or_prefix_only_font_refs() {
        let td = TempDir::new().unwrap();
        let config = td.path().join("ghostty.conf");
        let managed = td.path().join("managed/ghostty/font.conf");
        std::fs::create_dir_all(managed.parent().unwrap()).unwrap();
        std::fs::write(
            &config,
            format!(
                "# config-file = \"{}\"\nconfig-file = \"{}-old\"\nconfig-file = \"/tmp{}\"\ninclude = \"{}/child\"\n",
                managed.display(),
                managed.display(),
                managed.display(),
                managed.display()
            ),
        )
        .unwrap();

        assert!(!file_contains_managed_ref(
            &config,
            &managed,
            TerminalRefSyntax::Ghostty
        ));
    }

    #[test]
    fn font_apply_report_ignores_non_include_lines_with_managed_paths() {
        let td = TempDir::new().unwrap();
        let env = SlateEnv::with_home(td.path().to_path_buf());
        let ghostty_dir = env.xdg_config_home().join("ghostty");
        let alacritty_dir = env.xdg_config_home().join("alacritty");
        let kitty_dir = env.xdg_config_home().join("kitty");
        std::fs::create_dir_all(&ghostty_dir).unwrap();
        std::fs::create_dir_all(&alacritty_dir).unwrap();
        std::fs::create_dir_all(&kitty_dir).unwrap();

        let ghostty_font = env.config_dir().join("managed/ghostty/font.conf");
        let alacritty_font = env.config_dir().join("managed/alacritty/font.toml");
        let kitty_font = env.config_dir().join("managed/kitty/font.conf");

        std::fs::write(
            ghostty_dir.join("config.ghostty"),
            format!("note = \"{}\"\n", ghostty_font.display()),
        )
        .unwrap();
        std::fs::write(
            alacritty_dir.join("alacritty.toml"),
            format!("[general]\nnotes = [\"{}\"]\n", alacritty_font.display()),
        )
        .unwrap();
        std::fs::write(
            kitty_dir.join("kitty.conf"),
            format!("font_note {}\n", kitty_font.display()),
        )
        .unwrap();

        let report = collect_font_apply_report(&env);

        assert!(report.applied.is_empty());
        assert!(report
            .skipped
            .contains(&("Ghostty", "no Slate font include found")));
        assert!(report
            .skipped
            .contains(&("Alacritty", "no Slate font import found")));
        assert!(report
            .skipped
            .contains(&("Kitty", "no Slate font include found")));
    }

    #[test]
    fn font_apply_report_marks_alacritty_import_as_applied() {
        let td = TempDir::new().unwrap();
        let env = SlateEnv::with_home(td.path().to_path_buf());
        let alacritty_dir = env.xdg_config_home().join("alacritty");
        std::fs::create_dir_all(&alacritty_dir).unwrap();

        let alacritty_font = env.config_dir().join("managed/alacritty/font.toml");
        std::fs::write(
            alacritty_dir.join("alacritty.toml"),
            format!("[general]\nimport = [\"{}\"]\n", alacritty_font.display()),
        )
        .unwrap();

        let report = collect_font_apply_report(&env);

        assert_eq!(report.applied, vec!["Alacritty"]);
    }

    /// snapshot — `slate font <name>` success confirmation line
    /// rendered in Basic mode. Byte-locks the envelope shape
    /// (`✓ … in Slate-managed terminal configs.`).
    #[test]
    fn font_updated_success_basic_snapshot() {
        let theme = mock_theme();
        let ctx = mock_context_with_mode(&theme, RenderMode::Basic);
        let r = Roles::new(&ctx);
        let out = format_font_updated(Some(&r), "JetBrains Mono Nerd Font");
        insta::assert_snapshot!("font_updated_success_basic", out);
    }

    /// snapshot — catalog-download completion line.
    #[test]
    fn font_downloaded_basic_snapshot() {
        let theme = mock_theme();
        let ctx = mock_context_with_mode(&theme, RenderMode::Basic);
        let r = Roles::new(&ctx);
        let out = format_font_downloaded(Some(&r), "Hack Nerd Font");
        insta::assert_snapshot!("font_downloaded_basic", out);
    }

    /// D-01a — the download-failed line uses `Roles::status_error`
    /// (theme.red — NEVER brand lavender). Asserts the lavender byte
    /// triple (`38;2;114;135;253`, from `BRAND_LAVENDER_FIXED`) is
    /// absent across Truecolor / Basic / None.
    #[test]
    fn font_download_failed_never_emits_lavender() {
        let theme = mock_theme();
        for mode in [RenderMode::Truecolor, RenderMode::Basic, RenderMode::None] {
            let ctx = mock_context_with_mode(&theme, mode);
            let r = Roles::new(&ctx);
            let out = format_font_download_failed(Some(&r), "connection reset");
            assert!(
                !out.contains("38;2;114;135;253"),
                "D-01a violation in mode {mode:?}: {out:?}"
            );
        }
    }

    /// graceful degrade — every formatter emits pure plain text
    /// when Roles is absent. Confirms no ANSI bytes leak and the
    /// legacy glyph prefix stays identical to pre-Wave-2 output so
    /// users hitting the registry-init edge case see the same words.
    #[test]
    fn font_formatters_fall_back_to_plain_when_roles_absent() {
        let updated = format_font_updated(None, "Hack Nerd Font");
        let downloaded = format_font_downloaded(None, "Hack Nerd Font");
        let failed = format_font_download_failed(None, "connection reset");
        let empty = format_no_fonts_found(None);
        for out in [&updated, &downloaded, &failed, &empty] {
            assert!(!out.contains('\x1b'), "expected no ANSI bytes: {out:?}");
        }
        assert!(updated.starts_with("✓ Updated font to "));
        assert_eq!(downloaded, "✓ Hack Nerd Font downloaded");
        assert_eq!(failed, "✗ Download failed: connection reset");
        assert!(empty.starts_with("✗ No supported fonts found."));
    }
}
