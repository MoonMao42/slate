use crate::adapter::font::{FontAdapter, FontDiscovery};
use crate::cli::ui_language::tr;

use crate::brand::render_context::RenderContext;
use crate::brand::roles::Roles;
use crate::cli::font_selection::FontCatalog;
use crate::env::SlateEnv;
use crate::error::{Result, SlateError};
use crate::platform::fonts::FontCacheRefresh;
#[cfg(test)]
use std::path::Path;

mod apply;
mod choices;
mod commit;
mod listing;
mod preview;
mod suggestions;
pub use listing::{handle_list, handle_list_with_query, validate_list_query};
pub use preview::handle_preview;

#[cfg(test)]
#[path = "font/reference_tests.rs"]
mod reference_tests;

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

#[cfg(test)]
use crate::adapter::font::references::Terminal as TerminalRefSyntax;

impl ResolvedFontChoice {
    pub(crate) fn font_name(&self) -> &str {
        match self {
            Self::Installed(name) | Self::Catalog(name) => name,
        }
    }
}

fn find_installed_font(
    discovery: &FontDiscovery,
    name: &str,
    requested_key: &str,
) -> Result<Option<String>> {
    let candidates: Vec<_> = discovery
        .nerd_fonts
        .iter()
        .chain(discovery.system_fonts.iter())
        .filter(|font| crate::adapter::font_config::validate_family(font).is_ok())
        .collect();
    // Punctuation and spacing can distinguish real families. Exact requests
    // must win before loose CLI alias matching, independent of discovery order.
    if let Some(font) = candidates.iter().find(|font| font.as_str() == name) {
        return Ok(Some((*font).to_owned()));
    }
    let matches: std::collections::BTreeSet<_> = candidates
        .into_iter()
        .filter(|font| FontAdapter::family_match_key(font) == requested_key)
        .collect();
    if matches.len() > 1 {
        return Err(suggestions::ambiguous(
            matches.into_iter().map(String::as_str),
        ));
    }
    Ok(matches.into_iter().next().cloned())
}

fn resolve_font_choice_with_discovery(
    name: &str,
    discovery: &FontDiscovery,
) -> Result<ResolvedFontChoice> {
    crate::adapter::font_config::validate_family(name)?;
    let requested_key = FontAdapter::family_match_key(name);

    if let Some(installed) = find_installed_font(discovery, name, &requested_key)? {
        return Ok(ResolvedFontChoice::Installed(installed));
    }

    if let Some(catalog_font) = FontCatalog::all_fonts().into_iter().find(|font| {
        font.name == name
            || font.id == name
            || FontAdapter::family_match_key(font.name) == requested_key
            || FontAdapter::family_match_key(font.id) == requested_key
    }) {
        let canonical_key = FontAdapter::family_match_key(catalog_font.name);
        if let Some(installed) = find_installed_font(discovery, catalog_font.name, &canonical_key)?
        {
            return Ok(ResolvedFontChoice::Installed(installed));
        }

        return Ok(ResolvedFontChoice::Catalog(catalog_font.name.to_string()));
    }

    Err(suggestions::not_found(name, discovery))
}

pub(crate) fn resolve_font_choice(name: &str) -> Result<ResolvedFontChoice> {
    crate::adapter::font_config::validate_family(name)?;
    let env = SlateEnv::from_process()?;
    resolve_font_choice_with_scan(name, &FontAdapter::scan_fonts_with_env(&env))
}

fn resolve_font_choice_with_scan(
    name: &str,
    scan: &crate::adapter::font::FontScanReport,
) -> Result<ResolvedFontChoice> {
    let choice = resolve_font_choice_with_discovery(name, &scan.fonts);
    if !matches!(choice, Ok(ResolvedFontChoice::Installed(_))) {
        scan.require_complete()?;
    }
    choice
}

fn revalidate_picker_choice(
    selection: &ResolvedFontChoice,
    scan: &crate::adapter::font::FontScanReport,
) -> Result<()> {
    let current = resolve_font_choice_with_scan(selection.font_name(), scan)?;
    if current != *selection {
        return Err(SlateError::InvalidConfig(
            "Font availability changed while the picker was open. Reopen the font menu and choose again; no installation or font configuration change was attempted.".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
fn file_contains_managed_ref(path: &Path, managed_path: &Path, syntax: TerminalRefSyntax) -> bool {
    let env = SlateEnv::with_home(path.parent().expect("test file parent").to_owned());
    crate::adapter::font::references::inspect_entry(&env, path, managed_path, syntax).state
        == crate::adapter::font::references::State::Found
}

fn collect_font_apply_report(env: &SlateEnv) -> FontApplyReport {
    use crate::adapter::font::references::{self, State};
    let mut report = FontApplyReport {
        applied: Vec::new(),
        skipped: Vec::new(),
    };
    for terminal in references::inspect(env) {
        let name = terminal.terminal.name();
        let reason = match terminal.state() {
            State::Found => {
                report.applied.push(name);
                continue;
            }
            State::Uninspectable => "entry config could not be inspected",
            State::Missing => terminal.terminal.missing_reason(),
            State::NotFound => terminal.terminal.unlinked_reason(),
        };
        report.skipped.push((name, reason));
    }
    report
}

/// Handle `slate font` command
/// Supports two modes:
/// 1. `slate font <name>` — Apply explicit font directly
/// 2. `slate font` (no args) — Launch interactive font picker with Nerd + System groups
pub fn handle_font(font_name: Option<&str>) -> Result<()> {
    handle_font_with_env(font_name, &SlateEnv::from_process()?, false, false)
}

pub fn validate_storage_paths(env: &SlateEnv) -> Result<()> {
    crate::config::recovery_paths::validate_storage_paths(env, "Font")
}

pub fn handle_font_with_env(
    font_name: Option<&str>,
    env: &SlateEnv,
    auto: bool,
    quiet: bool,
) -> Result<()> {
    handle_with_policy(font_name, env, auto, quiet, true)
}

pub(crate) fn handle_import_font(env: &SlateEnv, name: &str) -> Result<()> {
    handle_with_policy(Some(name), env, false, false, false)
}

fn handle_with_policy(
    font_name: Option<&str>,
    env: &SlateEnv,
    auto: bool,
    quiet: bool,
    snapshot: bool,
) -> Result<()> {
    if let Some(name) = font_name {
        crate::adapter::font_config::validate_family(name)?;
    }
    validate_storage_paths(env)?;
    // Build a RenderContext up front so every status line in this
    // handler shares the same byte contract (sketch 003 canon +
    // daily chrome). graceful degrade: plain text when
    // the theme registry cannot boot.
    let ctx = RenderContext::from_active_theme().ok();
    let roles = ctx.as_ref().map(Roles::new);

    if let Some(name) = font_name {
        // Direct apply path: validate and apply font
        let _write_guard = crate::config::ConfigWriteGuard::acquire(env)?;
        let selection =
            resolve_font_choice_with_scan(name, &FontAdapter::scan_fonts_with_env(env))?;
        apply::apply_choice(
            &selection,
            env,
            roles.as_ref(),
            apply::Options {
                auto,
                quiet,
                snapshot,
            },
            false,
        )
    } else {
        // Picker path: show font picker UI
        show_font_picker(roles.as_ref(), env, auto, quiet, snapshot)
    }
}

/// Format `✓ <font> downloaded` — success line emitted after a catalog
/// download completes. Routes through `Roles::status_success` so the ✓
/// glyph carries theme.green (never lavender per D-01a).
fn format_font_downloaded(r: Option<&Roles<'_>>, font_name: &str) -> String {
    let font_name = choices::terminal_text(font_name);
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
    let font_name = choices::terminal_text(font_name);
    match r {
        Some(r) => r.status_success(&format!(
            "Updated font to {} in Slate-managed terminal configs.",
            r.path(&font_name)
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
    let line = format!("Terminal font refs: observed for {applied}; not observed for {skipped}. Effective font not verified.");

    Some(match r {
        Some(r) => r.path(&line),
        None => format!("(i) {line}"),
    })
}

fn compact_font_apply_report(report: &FontApplyReport) -> String {
    let mut text = if report.applied.is_empty() {
        tr(
            "尚未发现终端引用此字体配置；用 slate doctor font 检查接入。",
            "No terminal references to this font configuration found; check slate doctor font.",
        )
        .to_owned()
    } else {
        format!(
            "{}{}{}",
            tr("字体配置已接入：", "Font configuration connected: "),
            report.applied.join(tr("、", ", ")),
            tr("；实际字体未检查。", "; effective font unverified.")
        )
    };
    let unreadable: Vec<_> = report
        .skipped
        .iter()
        .filter(|(_, reason)| *reason == "entry config could not be inspected")
        .map(|(name, _)| *name)
        .collect();
    if !unreadable.is_empty() {
        text.push_str(&format!(
            "\n{}{}{}",
            tr("无法检查：", "Could not inspect: "),
            unreadable.join(tr("、", ", ")),
            tr(
                "；用 slate doctor font 查看原因。",
                "; see slate doctor font for details."
            )
        ));
    }
    text
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

/// Show discovered candidates and catalog downloads from the shared list model.
fn show_font_picker(
    roles: Option<&Roles<'_>>,
    env: &SlateEnv,
    auto: bool,
    quiet: bool,
    snapshot: bool,
) -> Result<()> {
    let scan = FontAdapter::scan_fonts_with_env(env);
    if !scan.is_complete() {
        eprintln!(
            "Warning: {} Catalog downloads are hidden until the scan is complete.",
            scan.warning()
        );
    }
    let picker_items = choices::Choices::from_scan(&scan).picker_items();

    if picker_items.is_empty() {
        scan.require_complete()?;
        eprintln!("{}", format_no_fonts_found(roles));
        return Ok(());
    }

    // Launch picker
    cliclack::intro(tr("✦ 更换字体", "✦ Change Font"))?;

    let saved = crate::config::ConfigManager::from_env_paths(env).get_current_font();
    let mut last_selection = match saved {
        Ok(Some(family)) => {
            let key = choices::saved_key(&picker_items, &family);
            let note = if key.is_some() {
                tr("实际字体未检查", "Effective font unverified")
            } else {
                tr(
                    "本次扫描未唯一匹配到已安装字体；不会自动下载",
                    "No unique installed match in this scan; no automatic download",
                )
            };
            super::file_output::write_required(&format!(
                "{}{} · {note}\n",
                tr("已保存字体：", "Saved font: "),
                choices::terminal_text(&family)
            ))?;
            key
        }
        Ok(None) => None,
        Err(_) => {
            super::file_output::write_required(
                tr("无法读取已保存字体；可继续浏览，保存前需修复配置。\n", "Saved font is unreadable; browsing is available, but fix configuration before saving.\n"),
            )?;
            None
        }
    };
    let item = loop {
        let mut menu_builder =
            super::menu::select(tr("选择字体：", "Select font:")).escape_value("back");
        if let Some(key) = last_selection {
            menu_builder = menu_builder.initial_value(key);
        }
        for item in &picker_items {
            menu_builder = menu_builder.item(item.key.as_str(), item.label.as_str(), item.hint);
        }

        let selected = menu_builder
            .item("back", tr("保留当前字体", "Keep Current Font"), "")
            .interact()?;
        if selected == "back" {
            return Ok(());
        }
        last_selection = Some(selected);

        let item = picker_items
            .iter()
            .find(|item| item.key == selected)
            .ok_or_else(|| {
                SlateError::Internal("Selected font is no longer in the picker model.".into())
            })?;
        let (verb, cancel, confirm, hint) = if item.needs_install {
            (
                tr("下载并使用", "Download and Use"),
                tr("暂不下载", "Cancel Download"),
                tr("确认下载并使用", "Download and Use"),
                tr(
                    "联网安装字体并更新 Slate 管理的终端字体配置",
                    "Download the font and update Slate-managed terminal font configuration",
                ),
            )
        } else {
            (
                tr("使用", "Use"),
                tr("暂不更换", "Cancel"),
                tr("确认使用", "Use Font"),
                tr(
                    "更新 Slate 管理的终端字体配置；不下载字体",
                    "Update Slate-managed terminal font configuration; no download",
                ),
            )
        };
        let confirmed = {
            loop {
                let action = super::menu::select(format!(
                    "{verb} {}{}",
                    choices::terminal_text(&item.family),
                    tr("？", "?")
                ))
                .initial_value("back")
                .escape_value("back")
                .item("back", cancel, "")
                .item(
                    "preview",
                    tr("预览配置改动", "Preview Configuration Changes"),
                    tr(
                        "只查看计划；不联网、不安装、不写文件",
                        "Preview only; no network, installation or writes",
                    ),
                )
                .item("apply", confirm, hint)
                .interact()?;
                match action {
                    "preview" => preview::handle_menu_preview(env, &item.family)?,
                    "apply" => break true,
                    _ => break false,
                }
            }
        };
        if !confirmed {
            continue;
        }
        break item;
    };
    // Display escaping and badges never become family data.
    let selection = if item.needs_install {
        ResolvedFontChoice::Catalog(item.family.clone())
    } else {
        ResolvedFontChoice::Installed(item.family.clone())
    };
    // Browsing/declining is read-only and must not hold the cooperative writer
    // lock. Recheck availability after acquiring it so a disappeared installed
    // font cannot silently turn into a download without fresh consent.
    let _write_guard = crate::config::ConfigWriteGuard::acquire(env)?;
    revalidate_picker_choice(&selection, &FontAdapter::scan_fonts_with_env(env))?;
    apply::apply_choice(
        &selection,
        env,
        roles,
        apply::Options {
            auto,
            quiet,
            snapshot,
        },
        true,
    )
}

/// Setup and selection share the same ordered installation policy.
fn download_catalog_font(
    font_name: &str,
    env: &SlateEnv,
) -> std::result::Result<FontCacheRefresh, String> {
    crate::cli::setup_executor::install_catalog_font(font_name, env, |_| {})
        .map(|report| {
            // Recovered failures stay visible even in quiet mode, before a
            // later configuration commit can fail independently.
            for notice in report.notices {
                eprintln!("Warning: {}", choices::terminal_text(&notice));
            }
            report.cache
        })
        .map_err(|error| {
            let full = error.to_string();
            full.strip_prefix("Internal error: ")
                .unwrap_or(&full)
                .to_owned()
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
    use crate::env::SlateEnv;
    use tempfile::TempDir;

    #[test]
    fn picker_revalidation_never_converts_installed_selection_into_download() {
        let family = "Hack Nerd Font".to_owned();
        let mut scan = crate::adapter::font::FontScanReport {
            fonts: FontDiscovery {
                nerd_fonts: vec![family.clone()],
                system_fonts: vec![],
            },
            issues: vec![],
            omitted_issues: 0,
        };
        let installed = ResolvedFontChoice::Installed(family.clone());
        let catalog = ResolvedFontChoice::Catalog(family);
        assert!(super::revalidate_picker_choice(&installed, &scan).is_ok());
        assert!(super::revalidate_picker_choice(&catalog, &scan).is_err());
        scan.fonts.nerd_fonts.clear();
        assert!(super::revalidate_picker_choice(&installed, &scan)
            .unwrap_err()
            .to_string()
            .contains("availability changed"));
        assert!(super::revalidate_picker_choice(&catalog, &scan).is_ok());
        scan.issues.push(crate::adapter::font::FontScanIssue {
            path: "/private/font-root".into(),
            reason: "cannot list font directory",
        });
        assert!(super::revalidate_picker_choice(&catalog, &scan).is_err());
    }

    #[test]
    fn font_discovery_partial_scan_allows_known_choices_but_blocks_catalog_downloads() {
        use crate::adapter::font::{FontScanIssue, FontScanReport};
        let scan = FontScanReport {
            fonts: FontDiscovery {
                nerd_fonts: vec!["Known Nerd Font".into()],
                system_fonts: vec![],
            },
            issues: vec![FontScanIssue {
                path: "/private/font-root".into(),
                reason: "cannot list font directory",
            }],
            omitted_issues: 0,
        };
        assert_eq!(
            super::resolve_font_choice_with_scan("Known Nerd Font", &scan).unwrap(),
            ResolvedFontChoice::Installed("Known Nerd Font".into())
        );
        for name in ["jetbrains-mono", "Hack Nerd Font", "Unknown Nerd Font"] {
            let error = super::resolve_font_choice_with_scan(name, &scan)
                .unwrap_err()
                .to_string();
            assert!(error.contains("discovery is incomplete"), "{error}");
            assert!(!error.contains("not found"));
        }
    }

    #[test]
    fn exact_font_names_win_and_ambiguous_aliases_require_a_choice() {
        let discovery = FontDiscovery {
            nerd_fonts: vec!["Twin\"Mono Nerd Font".into(), "TwinMono Nerd Font".into()],
            system_fonts: vec![],
        };
        for name in [&discovery.nerd_fonts[0], &discovery.nerd_fonts[1]] {
            assert_eq!(
                resolve_font_choice_with_discovery(name, &discovery).unwrap(),
                ResolvedFontChoice::Installed(name.clone())
            );
        }
        assert!(
            resolve_font_choice_with_discovery("Twin Mono Nerd Font", &discovery)
                .unwrap_err()
                .to_string()
                .contains("ambiguous alias")
        );
        let duplicate = FontDiscovery {
            nerd_fonts: vec!["TwinMono Nerd Font".into()],
            system_fonts: vec!["TwinMono Nerd Font".into()],
        };
        assert_eq!(
            resolve_font_choice_with_discovery("twinmono-nerd-font", &duplicate).unwrap(),
            ResolvedFontChoice::Installed("TwinMono Nerd Font".into())
        );
        let unsafe_name = FontDiscovery {
            nerd_fonts: vec!["Twin\nMono Nerd Font".into()],
            system_fonts: vec![],
        };
        assert!(resolve_font_choice_with_discovery("TwinMono Nerd Font", &unsafe_name).is_err());
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
            "(i) Terminal font refs: observed for Ghostty; not observed for Kitty (missing kitty.conf). Effective font not verified."
        );
    }

    #[test]
    fn compact_font_report_omits_missing_tools_but_keeps_inspection_failures() {
        let mut report = FontApplyReport {
            applied: vec!["Ghostty"],
            skipped: vec![("Kitty", "missing kitty.conf")],
        };
        let text = super::compact_font_apply_report(&report);
        assert!(text.contains("Ghostty") && text.contains("实际字体未检查"));
        assert!(!text.contains("Kitty"));
        report
            .skipped
            .push(("Alacritty", "entry config could not be inspected"));
        let text = super::compact_font_apply_report(&report);
        assert!(text.contains("无法检查：Alacritty") && text.contains("slate doctor font"));
        report.applied.clear();
        let text = super::compact_font_apply_report(&report);
        assert!(text.contains("尚未发现终端引用") && text.contains("无法检查：Alacritty"));
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

    #[test]
    fn font_apply_report_checks_only_the_effective_alacritty_import() {
        use std::fs;
        let td = TempDir::new().unwrap();
        let env = SlateEnv::with_home(td.path().to_owned());
        let path = env.xdg_config_home().join("alacritty/alacritty.toml");
        let font = env.managed_file("managed/alacritty/font.toml");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        for root in ["[]", "['user.toml']", "42"] {
            fs::write(
                &path,
                format!(
                    "import = {root}\n[general]\nimport = ['{}']\n",
                    font.display()
                ),
            )
            .unwrap();
            assert!(!collect_font_apply_report(&env)
                .applied
                .contains(&"Alacritty"));
        }
        fs::write(
            &path,
            format!("import = ['{}']\n[general]\nimport = []\n", font.display()),
        )
        .unwrap();
        assert!(collect_font_apply_report(&env)
            .applied
            .contains(&"Alacritty"));
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
