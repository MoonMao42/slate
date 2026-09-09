use crate::adapter::{GhosttyAdapter, ToolAdapter};
use crate::brand::events::{dispatch, BrandEvent, FailureKind, SuccessKind};
use crate::brand::render_context::RenderContext;
use crate::brand::roles::Roles;
use crate::env::SlateEnv;
use crate::error::Result;
use crate::{config::ConfigManager, platform};
use std::ffi::OsStr;
use std::fs;
use std::path::Path;

mod edits;
mod preflight;
mod preview;

pub fn handle_preview(env: &SlateEnv, json: bool) -> Result<()> {
    preview::handle(env, json)
}

/// Called before main acquires its writer lock or initializes sound caches.
/// The full target preflight is repeated under the lock in the handler.
pub fn validate_storage_paths(env: &SlateEnv) -> Result<()> {
    preflight::validate_storage_paths(env)
}

/// Handle `slate clean` command
/// Removes managed files, stops the auto-theme watcher, and removes.zshrc marker block
/// Clean removes slate-managed assets; see 'slate restore' to recover from snapshot
pub fn handle_clean() -> Result<()> {
    match handle_clean_inner() {
        Ok(()) => Ok(()),
        Err(err) => {
            // clean-level failure → `BrandEvent::Failure(CleanFailed)`
            // so SoundSink maps the error moment to the failure
            // SFX. Paired Success events dispatch from the happy path below.
            dispatch(BrandEvent::Failure(FailureKind::CleanFailed));
            Err(err)
        }
    }
}

fn handle_clean_inner() -> Result<()> {
    use cliclack::{intro, log};
    let env = SlateEnv::from_process()?;
    validate_storage_paths(&env)?;
    let _write_guard = crate::config::ConfigWriteGuard::acquire(&env)?;

    // Build a RenderContext up-front so every user-visible status line
    // shares the same byte contract (sketch 003 tree narrative
    // daily chrome + D-01a severity). graceful degrade — plain text
    // when the theme registry fails to load.
    let ctx = RenderContext::from_active_theme().ok();
    let r = ctx.as_ref().map(Roles::new);

    intro(intro_title(r.as_ref(), "Clean Up Slate"))?;

    // Inspection must not initialize state before a recoverable snapshot exists.
    let config = ConfigManager::from_env_paths(&env);
    let targets = preflight::snapshot_targets(&env)?;

    let mut removed_sections: Vec<&'static str> = Vec::new();

    // Step 0: Snapshot the current state so the user can undo this clean. Without this
    // the only restore point after clean is the pre-slate baseline, which is the wrong
    // target if the user just wants to roll back the clean itself. Fail closed:
    // deleting files that were not backed up is not a successful cleanup.
    let snapshot = {
        let label = config
            .get_current_theme()
            .ok()
            .flatten()
            .map(|theme| format!("pre-clean-{}", theme))
            .unwrap_or_else(|| "pre-clean".to_string());
        let snapshot = crate::config::snapshot_clean_targets_with_env(&env, &label, &targets)
            .map_err(|err| {
                crate::error::SlateError::BackupFailed(format!(
                    "Clean cancelled before removing files: pre-clean snapshot failed: {err}"
                ))
            })?;
        log::success(status_success_line(
            r.as_ref(),
            &format!("Saved pre-clean snapshot: {}", snapshot.id),
        ))?;
        snapshot
    };

    // Any failure after the snapshot must include a usable recovery reference.
    let result = (|| -> Result<()> {
        // Step 1: Stop watcher + clear config flag
        log::step("Stopping auto-theme watcher...")?;
        if env.session().is_isolated() {
            log::remark("  (isolated profile: running watchers left untouched)")?;
        } else {
            config.set_auto_theme_enabled(false)?;
            platform::dark_mode_notify::stop_with_env(&env)?;
            log::success(status_success_line(r.as_ref(), "Watcher stopped"))?;
            removed_sections.push("auto-theme watcher");
        }
        // The managed binary is removed with the already-snapshotted managed tree.

        // Step 2: Remove integration references before deleting managed files
        log::step("Removing integration references...")?;
        crate::adapter::marker_block::remove_managed_blocks_from_file(&env.zshrc_path())?;
        remove_marker_blocks_from_bash(&env)?;
        remove_fish_loader(&env)?;
        remove_ghostty_managed_references(&env)?;
        remove_alacritty_managed_references(&env)?;
        remove_kitty_managed_references(&env)?;
        remove_starship_managed_references(&env)?;
        remove_tmux_managed_references(&env)?;
        remove_delta_managed_references(env.home())?;
        remove_nvim_managed_references(&env)?;
        remove_opencode_managed_references(&env)?;
        edits::apply(&crate::adapter::BtopAdapter::config_path(&env), |bytes| {
            edits::btop(&env, bytes)
        })?;
        edits::apply(
            &crate::adapter::BtopAdapter::theme_path(&env),
            edits::btop_theme,
        )?;
        edits::apply(&crate::adapter::YaziAdapter::config_path(&env), edits::yazi)?;
        let [zellij_config, zellij_theme] = crate::adapter::ZellijAdapter::paths(&env)?;
        edits::apply(&zellij_config, edits::zellij)?;
        edits::apply(&zellij_theme, edits::zellij_theme)?;
        edits::apply(&crate::adapter::YaziAdapter::flavor_path(&env), |bytes| {
            edits::yazi_asset(bytes, false)
        })?;
        edits::apply(&crate::adapter::YaziAdapter::syntax_path(&env), |bytes| {
            edits::yazi_asset(bytes, true)
        })?;
        log::success(status_success_line(
            r.as_ref(),
            "Removed config-file/import/source hooks",
        ))?;
        removed_sections.push("shell + tool hooks");

        // Step 3: Delete Slate-owned config directory
        log::step("Removing Slate-managed config state...")?;
        if remove_slate_owned_config_state(&env)? {
            log::success(status_success_line(
                r.as_ref(),
                "Removed Slate-owned config state",
            ))?;
            removed_sections.push("managed config state");
        } else if env.config_dir().exists() {
            log::remark(format!(
                "  (only {} remains)",
                env.config_dir().join("user").display()
            ))?;
        } else {
            log::remark(format!(
                "  ({} already removed)",
                env.config_dir().display()
            ))?;
        }

        Ok(())
    })();
    result.map_err(|err| crate::error::SlateError::InvalidConfig(format!(
        "Clean stopped: {err}. Some files may have changed. Inspect the saved files with `slate restore {} --dry-run`.",
        snapshot.id,
    )))?;

    // Step 4: Reload running terminals so the theme actually drops.
    // Removing the config-file line from ~/.config/ghostty/config.ghostty only takes effect on the
    // next reload; without this, users see "clean succeeded" but the background + palette
    // stay applied until they restart Ghostty themselves. Best-effort — if the terminal
    // isn't running we silently move on.
    if env.session().can_reload_terminal() {
        let _ = GhosttyAdapter.reload();
    }

    // completion receipt is a static tree-narrative anchor — bypass
    // cliclack and println! via Roles::heading / tree_branch / tree_end.
    // Sketch 003 canon: `◆ Cleanup summary ┃ ├─ … └─ ★ Ready for a fresh start`.
    println!();
    println!("{}", heading_text(r.as_ref(), "Cleanup summary"));
    for section in &removed_sections {
        println!(
            "{}",
            tree_branch_text(r.as_ref(), &format!("{} ✓", section))
        );
    }
    println!("{}", tree_end_text(r.as_ref(), "Ready for a fresh start"),);
    println!();

    // Exit message: Clarify clean vs restore boundary. Routed through
    // log::info so cliclack's lavender-bar SlateTheme renders the chrome
    // while the body is whatever the Language copy says today.
    log::info(format!(
        "clean removed Slate-owned shell hooks, watcher artifacts, and config state. \
Third-party tools installed through Homebrew remain installed. \
Inspect saved files with 'slate restore {} --dry-run' before restoring. \
File restoration does not restore running application state.",
        snapshot.id,
    ))?;

    // clean success → paired `CleanComplete` (category) +
    // `ApplyComplete` (whole-flow milestone) so can latch onto
    // either the per-category or per-command moment.
    dispatch(BrandEvent::Success(SuccessKind::CleanComplete));
    dispatch(BrandEvent::ApplyComplete);

    Ok(())
}

/// Build the intro header title. Always starts with the ✦ brand glyph
/// (routed through `Roles::brand` when available) so the wordmark keeps
/// the lavender anchor that Sketch 002 locks in.
fn intro_title(r: Option<&Roles<'_>>, text: &str) -> String {
    match r {
        Some(r) => format!("{} {}", r.brand("✦"), text),
        None => format!("✦ {}", text),
    }
}

/// Format a `log::success` body via `Roles::status_success` (theme.green
/// NEVER lavender per D-01a), falling back to plain `✓ message`.
fn status_success_line(r: Option<&Roles<'_>>, message: &str) -> String {
    match r {
        Some(r) => r.status_success(message),
        None => format!("✓ {}", message),
    }
}

/// Render `◆ title` via `Roles::heading`, falling back to plain ◆ text
/// when Roles is unavailable (graceful degrade).
fn heading_text(r: Option<&Roles<'_>>, title: &str) -> String {
    match r {
        Some(r) => r.heading(title),
        None => format!("◆ {}", title),
    }
}

/// Render `┃ ├─ text` via `Roles::tree_branch`.
fn tree_branch_text(r: Option<&Roles<'_>>, text: &str) -> String {
    match r {
        Some(r) => r.tree_branch(text),
        None => format!("┃ ├─ {}", text),
    }
}

/// Render `└─ ★ text` via `Roles::tree_end`.
fn tree_end_text(r: Option<&Roles<'_>>, text: &str) -> String {
    match r {
        Some(r) => r.tree_end(text),
        None => format!("└─ ★ {}", text),
    }
}

/// Remove marker blocks from any bash rc file Slate might have written to.
/// On macOS we may have written to a login entry; on Linux we write to `.bashrc`.
/// Sweep all supported entries so a reinstall across machines or a migration from
/// an older slate version still leaves no orphaned loaders. `remove_managed_blocks_from_file`
/// is a no-op on missing files, so unconditional calls are safe.
fn remove_marker_blocks_from_bash(env: &SlateEnv) -> Result<()> {
    for path in env.bash_startup_paths() {
        crate::adapter::marker_block::remove_managed_blocks_from_file(&path)?;
    }
    Ok(())
}

fn remove_fish_loader(env: &SlateEnv) -> Result<()> {
    match fs::remove_file(env.fish_loader_path()) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err.into()),
    }
}

fn remove_slate_owned_config_state(env: &SlateEnv) -> Result<bool> {
    let config_dir = env.config_dir();
    if !config_dir.exists() {
        return Ok(false);
    }

    let mut removed_any = false;
    for entry in fs::read_dir(config_dir)? {
        let entry = entry?;
        if entry.file_name().as_os_str() == OsStr::new("user") {
            continue;
        }

        let path = entry.path();
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            fs::remove_dir_all(&path)?;
        } else {
            fs::remove_file(&path)?;
        }
        removed_any = true;
    }

    if !config_dir.join("user").exists() && fs::read_dir(config_dir)?.next().is_none() {
        fs::remove_dir(config_dir)?;
    }

    Ok(removed_any)
}

/// Strip slate's in-place edits from the user's `starship.toml`.
/// StarshipAdapter (unlike the WriteAndInclude adapters) modifies the
/// user's integration file directly — setting `palette = "slate"` at the
/// root and injecting a `[palettes.slate]` table with the active theme's
/// colors. Clean must undo both, otherwise the starship prompt stays
/// themed even after every slate file is deleted (issue #3 tail):
/// - `palette = "slate"` only reverted when currently set to "slate"
/// (we can't restore the user's pre-slate palette — that's
/// `slate restore <baseline>`'s job).
/// - `[palettes.slate]` table removed unconditionally.
/// - empty `[palettes]` table removed after cleanup.
/// Uses the same integration path as apply and backup. STARSHIP_CONFIG may
/// point at a generated fallback or an unrelated file and must not select a
/// different cleanup target.
/// Leaves non-UTF-8 or unparseable files unchanged with a warning.
fn remove_starship_managed_references(env: &SlateEnv) -> Result<()> {
    let integration_path = crate::adapter::StarshipAdapter::integration_config_path_with_env(env);
    strip_starship_slate_palette(&integration_path)
}

/// Path-level write wrapper around the same pure transform used by preview.
fn strip_starship_slate_palette(integration_path: &Path) -> Result<()> {
    edits::apply(integration_path, edits::starship)
}

/// Strip slate-owned lines from `~/.config/kitty/kitty.conf`.
/// Only removes lines that slate can positively identify as slate-owned:
/// - `include` lines pointing anywhere under `~/.config/slate/managed/kitty/`
/// (theme.conf, opacity.conf, font.conf)
/// - `listen_on` directives with an absolute Unix socket path whose exact
/// basename is "kitty-slate" — `listen_on` was prepended by the live-preview
/// wiring in kitty.rs to enable `kitten @ set-colors`.
/// Lines slate also prepends but that the user may legitimately want to
/// keep (`allow_remote_control socket-only`, `dynamic_background_opacity yes`)
/// are NOT stripped — they don't carry a slate-specific marker and the
/// safer posture is to leave them in place than to nuke a pre-existing
/// user setting.
/// Whole continuation groups are edited; all retained bytes are preserved.
fn remove_kitty_managed_references(env: &SlateEnv) -> Result<()> {
    let path = crate::adapter::KittyAdapter::resolve_config_path_with_env(env);
    edits::apply(&path, |bytes| edits::kitty(env, bytes))
}

fn remove_ghostty_managed_references(env: &SlateEnv) -> Result<()> {
    let adapter = crate::adapter::GhosttyAdapter;
    for integration_path in adapter.integration_candidate_paths_with_env(env)? {
        crate::adapter::GhosttyAdapter::strip_managed_references_from_path(env, &integration_path)?;
    }

    Ok(())
}

fn remove_alacritty_managed_references(env: &SlateEnv) -> Result<()> {
    for path in crate::adapter::AlacrittyAdapter::integration_candidate_paths_with_env(env) {
        edits::apply(&path, |bytes| edits::alacritty(env, bytes))?;
    }
    Ok(())
}

fn remove_tmux_managed_references(env: &SlateEnv) -> Result<()> {
    for path in env.tmux_config_candidates() {
        crate::adapter::marker_block::remove_managed_blocks_from_file(&path)?;
    }
    Ok(())
}

fn remove_delta_managed_references(home: &Path) -> Result<()> {
    let gitconfig_path = home.join(".gitconfig");
    crate::adapter::marker_block::remove_managed_blocks_from_file(&gitconfig_path)
}

/// Best-effort strip of Slate's OpenCode TUI theme selection. Slate only sets
/// `theme = "system"` (plus the standard schema when creating a new file), so
/// clean removes that value and preserves unrelated user settings.
fn remove_opencode_managed_references(env: &SlateEnv) -> Result<()> {
    for tui_path in crate::adapter::OpencodeAdapter::tui_config_paths(env) {
        strip_opencode_slate_theme(&tui_path)?;
    }

    Ok(())
}

fn strip_opencode_slate_theme(tui_path: &Path) -> Result<()> {
    edits::apply(tui_path, |bytes| edits::opencode(bytes, tui_path))
}

/// Remove every slate-owned file under `~/.config/nvim/` plus the
/// state file in `~/.cache/slate/`, and best-effort strip the
/// `pcall(require, 'slate')` marker block from init.lua / init.vim.
/// Non-slate files in `colors/` are preserved.
/// Missing entries are harmless; IO failures must reach the command's saved
/// snapshot receipt rather than producing a false successful cleanup.
fn remove_nvim_managed_references(env: &SlateEnv) -> Result<()> {
    let nvim_home = env.nvim_config_dir();

    // 1. Remove every `slate-*.lua` shim under ~/.config/nvim/colors/.
    // User-owned files (my-custom.lua, theme.lua, …) are preserved
    // Pitfall 7 guard verified by
    // `remove_nvim_managed_references_leaves_user_files_alone`.
    let colors_dir = nvim_home.join("colors");
    if colors_dir.exists() {
        for entry in fs::read_dir(&colors_dir)? {
            let entry = entry?;
            let name = entry.file_name();
            if preflight::is_nvim_shim(&name) {
                fs::remove_file(entry.path())?;
            }
        }
    }

    // 2. Remove the loader dir ~/.config/nvim/lua/slate/ (slate-owned).
    let loader_dir = nvim_home.join("lua").join("slate");
    if loader_dir.exists() {
        fs::remove_dir_all(&loader_dir)?;
    }

    // 3. Strip the marker block from both init files (no-op when missing).
    crate::adapter::marker_block::remove_managed_blocks_from_file(&nvim_home.join("init.lua"))?;
    crate::adapter::marker_block::remove_managed_blocks_from_file(&nvim_home.join("init.vim"))?;

    // 4. Remove the state file ~/.cache/slate/current_theme.lua.
    // `Step 3: Remove Slate-managed config state` in handle_clean
    // deletes the whole ~/.config/slate/ tree but the nvim state
    // file lives under ~/.cache/slate/, so the explicit removal
    // here guarantees no orphan state file survives.
    let state_file = env.slate_cache_dir().join("current_theme.lua");
    if state_file.exists() {
        fs::remove_file(&state_file)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::brand::render_context::{mock_context_with_mode, mock_theme, RenderMode};
    use crate::theme::ThemeRegistry;
    use tempfile::TempDir;

    #[test]
    fn session_context_tmux_paths_and_cleanup_preserve_user_config() {
        let td = TempDir::new().unwrap();
        let env = SlateEnv::from_vars(|key| match key {
            "HOME" => Some(td.path().as_os_str().to_owned()),
            "XDG_CONFIG_HOME" => Some(td.path().join("custom config").into_os_string()),
            _ => None,
        })
        .unwrap();
        let candidates = env.tmux_config_candidates();
        assert_eq!(candidates.len(), 3);
        assert_eq!(env.tmux_config_path(), candidates[0]);
        for candidate in candidates.iter().rev() {
            fs::create_dir_all(candidate.parent().unwrap()).unwrap();
            fs::write(
                candidate,
                format!(
                    "set -g mouse on\n{}\nsource-file '/managed/colors.conf'\n{}\nset -g status-position top\n",
                    crate::adapter::marker_block::START,
                    crate::adapter::marker_block::END,
                ),
            )
            .unwrap();
            assert_eq!(env.tmux_config_path(), *candidate);
        }
        remove_tmux_managed_references(&env).unwrap();
        for candidate in candidates {
            let content = fs::read_to_string(candidate).unwrap();
            assert!(content.contains("set -g mouse on"));
            assert!(content.contains("set -g status-position top"));
            assert!(!content.contains("source-file"));
            assert!(!content.contains(crate::adapter::marker_block::START));
        }
    }

    #[test]
    fn custom_paths_setup_backup_restore_and_clean_use_the_same_profile() {
        let td = TempDir::new().unwrap();
        let env = SlateEnv::from_vars(|key| match key {
            "HOME" => Some(td.path().as_os_str().to_owned()),
            "XDG_CONFIG_HOME" => Some(td.path().join("config root").into_os_string()),
            "XDG_CACHE_HOME" => Some(td.path().join("cache root").into_os_string()),
            "ZDOTDIR" => Some(td.path().join("shell").into_os_string()),
            "NVIM_APPNAME" => Some("profiles/work".into()),
            _ => None,
        })
        .unwrap();
        fs::create_dir_all(env.nvim_config_dir()).unwrap();
        fs::create_dir_all(env.zshrc_path().parent().unwrap()).unwrap();
        let init = env.nvim_config_dir().join("init.vim");
        fs::write(&init, "set number\n").unwrap();
        fs::write(env.zshrc_path(), "# user shell\n").unwrap();
        let baseline = crate::config::begin_restore_point_baseline_with_env(&env).unwrap();
        // Refuse to exercise restore if any target escapes this fixture.
        assert!(baseline
            .entries
            .iter()
            .all(|entry| entry.original_path.starts_with(td.path())));
        assert!(baseline
            .entries
            .iter()
            .any(|entry| entry.original_path == init));
        assert!(baseline
            .entries
            .iter()
            .any(|entry| entry.original_path == env.zshrc_path()));

        let registry = ThemeRegistry::new().unwrap();
        crate::adapter::NvimAdapter::setup(&env, registry.get("catppuccin-mocha").unwrap())
            .unwrap();
        crate::cli::setup::apply_activation_choice_a(&env).unwrap();
        assert!(fs::read_to_string(&init)
            .unwrap()
            .contains("lua pcall(require, 'slate')"));
        assert!(!env.nvim_config_dir().join("init.lua").exists());
        let loader = fs::read_to_string(env.nvim_config_dir().join("lua/slate/init.lua")).unwrap();
        assert!(loader.contains(&format!(
            "local STATE_PATH = {:?}",
            env.slate_cache_dir()
                .join("current_theme.lua")
                .to_string_lossy()
        )));
        assert!(!td.path().join(".config/nvim").exists());

        crate::config::execute_restore_with_env(&env, &baseline.id).unwrap();
        assert_eq!(fs::read_to_string(&init).unwrap(), "set number\n");
        assert_eq!(
            fs::read_to_string(env.zshrc_path()).unwrap(),
            "# user shell\n"
        );
        let user_theme = env.nvim_config_dir().join("colors/my-theme.lua");
        fs::write(&user_theme, "-- user theme\n").unwrap();
        remove_nvim_managed_references(&env).unwrap();
        assert!(user_theme.exists());
        assert!(!env.nvim_config_dir().join("lua/slate").exists());
        assert!(!env.slate_cache_dir().join("current_theme.lua").exists());
        assert_eq!(fs::read_to_string(&init).unwrap(), "set number\n");
    }

    /// Helper: render the completion tree receipt against the given
    /// RenderMode. Mirrors the `println!` block inside `handle_clean_inner`
    /// so the snapshot can lock the exact byte shape without driving the
    /// whole clean flow.
    fn render_clean_receipt(r: Option<&Roles<'_>>, sections: &[&'static str]) -> String {
        let mut out = String::new();
        out.push('\n');
        out.push_str(&heading_text(r, "Cleanup summary"));
        out.push('\n');
        for section in sections {
            out.push_str(&tree_branch_text(r, &format!("{} ✓", section)));
            out.push('\n');
        }
        out.push_str(&tree_end_text(r, "Ready for a fresh start"));
        out.push('\n');
        out
    }

    /// snapshot — byte-lock the `slate clean` completion tree in
    /// Basic mode so the sketch-003 tree narrative stays stable across
    /// CI and contributor workstations (MockTheme).
    #[test]
    fn clean_summary_basic_snapshot() {
        let theme = mock_theme();
        let ctx = mock_context_with_mode(&theme, RenderMode::Basic);
        let r = Roles::new(&ctx);
        let out = render_clean_receipt(
            Some(&r),
            &[
                "auto-theme watcher",
                "shell + tool hooks",
                "managed config state",
            ],
        );
        insta::assert_snapshot!("clean_summary_basic", out);
    }

    /// Truecolor variant — anchors every tree glyph to the lavender
    /// brand byte triple (`38;2;114;135;253`) per Sketch 002.
    #[test]
    fn clean_summary_truecolor_snapshot() {
        let theme = mock_theme();
        let ctx = mock_context_with_mode(&theme, RenderMode::Truecolor);
        let r = Roles::new(&ctx);
        let out = render_clean_receipt(
            Some(&r),
            &[
                "auto-theme watcher",
                "shell + tool hooks",
                "managed config state",
            ],
        );
        assert!(
            out.contains("38;2;114;135;253"),
            "tree chrome must carry brand-lavender bytes in truecolor, got: {out:?}"
        );
        insta::assert_snapshot!("clean_summary_truecolor", out);
    }

    /// graceful degrade — without Roles the tree falls back to
    /// plain glyphs, zero ANSI bytes.
    #[test]
    fn clean_summary_falls_back_to_plain_when_roles_absent() {
        let out = render_clean_receipt(None, &["auto-theme watcher", "shell + tool hooks"]);
        assert!(
            !out.contains('\x1b'),
            "plain fallback must contain no ANSI bytes, got: {out:?}"
        );
        assert!(out.contains("◆ Cleanup summary"));
        assert!(out.contains("┃ ├─ auto-theme watcher ✓"));
        assert!(out.contains("└─ ★ Ready for a fresh start"));
    }

    /// D-01a invariant — `status_success_line` uses theme.green, never
    /// brand lavender, across every RenderMode.
    #[test]
    fn status_success_line_never_emits_brand_lavender() {
        let theme = mock_theme();
        for mode in [RenderMode::Truecolor, RenderMode::Basic, RenderMode::None] {
            let ctx = mock_context_with_mode(&theme, mode);
            let r = Roles::new(&ctx);
            let out = status_success_line(Some(&r), "Watcher stopped");
            assert!(
                !out.contains("38;2;114;135;253"),
                "D-01a violation in mode {mode:?}: {out:?}"
            );
        }
    }

    /// Full-install → clean contract: after running
    /// `NvimAdapter::setup` + writing a slate marker block to init.lua,
    /// `remove_nvim_managed_references` takes the filesystem back to
    /// the pre-install state — no `slate-*.lua` shims in colors/, no
    /// `lua/slate/` dir, no marker block in init.lua, no state file.
    #[test]
    fn remove_nvim_managed_references_removes_all_slate_files() {
        let td = TempDir::new().unwrap();
        let env = SlateEnv::with_home(td.path().to_path_buf());

        // Setup: run the real adapter install to seed 18 shims +
        // loader + state file. Any regression in `NvimAdapter::setup`
        // that adds a new managed path will surface here.
        let registry = ThemeRegistry::new().unwrap();
        let theme = registry.get("catppuccin-mocha").unwrap().clone();
        crate::adapter::NvimAdapter::setup(&env, &theme).unwrap();

        // Seed init.lua with a slate marker block (no Lua-comment
        // wrap required here — strip_managed_blocks is byte-positional
        // so the bare marker is sufficient for the clean contract;
        // the Lua-wrap only matters for *generating* valid init.lua).
        let init_lua = td.path().join(".config/nvim/init.lua");
        std::fs::create_dir_all(init_lua.parent().unwrap()).unwrap();
        let marker_block = format!(
            "{}\npcall(require, 'slate')\n{}\n",
            crate::adapter::marker_block::START,
            crate::adapter::marker_block::END,
        );
        std::fs::write(&init_lua, &marker_block).unwrap();

        // Exercise.
        remove_nvim_managed_references(&env).unwrap();

        // Assert: no slate-* files in colors/.
        let colors_dir = td.path().join(".config/nvim/colors");
        let slate_shims: Vec<_> = std::fs::read_dir(&colors_dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().starts_with("slate-"))
            .collect();
        assert_eq!(
            slate_shims.len(),
            0,
            "expected no slate-* shim files after clean, got {}",
            slate_shims.len()
        );

        // Assert: no loader dir.
        assert!(
            !td.path().join(".config/nvim/lua/slate").exists(),
            "lua/slate/ directory must be removed"
        );

        // Assert: marker block stripped from init.lua.
        let after = std::fs::read_to_string(&init_lua).unwrap();
        assert!(
            !after.contains(crate::adapter::marker_block::START),
            "marker START must be removed from init.lua"
        );
        assert!(
            !after.contains(crate::adapter::marker_block::END),
            "marker END must be removed from init.lua"
        );

        // Assert: no state file at ~/.cache/slate/current_theme.lua.
        assert!(
            !td.path().join(".cache/slate/current_theme.lua").exists(),
            "state file must be removed"
        );
    }

    /// Pitfall 7 guard: `remove_nvim_managed_references` must not
    /// touch user-owned files in `~/.config/nvim/colors/` — only
    /// entries whose filename starts with `slate-`. A user's custom
    /// `my-custom.lua` or `theme.lua` survives the clean.
    #[test]
    fn remove_nvim_managed_references_leaves_user_files_alone() {
        let td = TempDir::new().unwrap();
        let env = SlateEnv::with_home(td.path().to_path_buf());

        let colors_dir = td.path().join(".config/nvim/colors");
        std::fs::create_dir_all(&colors_dir).unwrap();

        // User's own colorscheme.
        let user_file = colors_dir.join("my-custom.lua");
        std::fs::write(&user_file, "vim.g.colors_name = 'my-custom'").unwrap();

        // Another user file with a slate-ish name but NOT prefixed
        // with `slate-` (e.g. `slatecolors.lua`, `not-slate.lua`).
        let edge = colors_dir.join("not-slate.lua");
        std::fs::write(&edge, "-- user").unwrap();

        // A genuine slate shim — should be removed.
        std::fs::write(
            colors_dir.join("slate-tokyo-night-dark.lua"),
            "require('slate').load('tokyo-night-dark')",
        )
        .unwrap();

        remove_nvim_managed_references(&env).unwrap();

        assert!(user_file.exists(), "my-custom.lua must survive clean");
        assert!(edge.exists(), "not-slate.lua must survive clean");
        assert!(
            !colors_dir.join("slate-tokyo-night-dark.lua").exists(),
            "slate shim must be removed"
        );
    }

    /// Missing-files contract: running clean on a pristine home with
    /// no nvim config must succeed silently. Matches `remove_fish_loader`'s
    /// NotFound posture.
    #[test]
    fn remove_nvim_managed_references_is_noop_on_empty_home() {
        let td = TempDir::new().unwrap();
        let env = SlateEnv::with_home(td.path().to_path_buf());
        assert!(remove_nvim_managed_references(&env).is_ok());
        // No side effects — no directory materialized.
        assert!(!td.path().join(".config/nvim").exists());
    }

    #[test]
    fn remove_opencode_managed_references_deletes_slate_only_tui_config() {
        let td = TempDir::new().unwrap();
        let env = SlateEnv::with_home(td.path().to_path_buf());
        let tui_path = td.path().join(".config/opencode/tui.json");
        std::fs::create_dir_all(tui_path.parent().unwrap()).unwrap();
        std::fs::write(
            &tui_path,
            serde_json::json!({
                "$schema": crate::adapter::OpencodeAdapter::TUI_SCHEMA,
                "theme": "system"
            })
            .to_string(),
        )
        .unwrap();

        remove_opencode_managed_references(&env).unwrap();

        assert!(
            !tui_path.exists(),
            "Slate-only OpenCode TUI config should be removed"
        );
    }

    #[test]
    fn remove_opencode_managed_references_preserves_user_settings() {
        let td = TempDir::new().unwrap();
        let env = SlateEnv::with_home(td.path().to_path_buf());
        let tui_path = td.path().join(".config/opencode/tui.json");
        std::fs::create_dir_all(tui_path.parent().unwrap()).unwrap();
        std::fs::write(
            &tui_path,
            serde_json::json!({
                "$schema": crate::adapter::OpencodeAdapter::TUI_SCHEMA,
                "theme": "system",
                "mouse": false,
                "scroll_speed": 5
            })
            .to_string(),
        )
        .unwrap();

        remove_opencode_managed_references(&env).unwrap();

        let after: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&tui_path).unwrap()).unwrap();
        assert!(after.get("theme").is_none());
        assert_eq!(after["mouse"], false);
        assert_eq!(after["scroll_speed"], 5);
    }

    #[test]
    fn remove_opencode_managed_references_handles_tui_jsonc() {
        let td = TempDir::new().unwrap();
        let env = SlateEnv::with_home(td.path().to_path_buf());
        let tui_path = td.path().join(".config/opencode/tui.jsonc");
        std::fs::create_dir_all(tui_path.parent().unwrap()).unwrap();
        std::fs::write(
            &tui_path,
            r#"{
                // user setting should survive
                "theme": "system",
                "mouse": false,
            }"#,
        )
        .unwrap();

        remove_opencode_managed_references(&env).unwrap();

        let after = std::fs::read_to_string(&tui_path).unwrap();
        crate::adapter::opencode::config::Document::parse(&after, &tui_path).unwrap();
        assert!(after.contains("// user setting should survive"));
        assert!(!after.contains("\"theme\""));
        assert!(after.contains("\"mouse\": false,"));
    }

    #[test]
    fn strip_starship_slate_palette_reverts_slate_palette_edits() {
        let td = TempDir::new().unwrap();
        let starship_path = td.path().join("starship.toml");

        let before = r##"
format = "$all"
palette = "slate"

[palettes.slate]
red = "#f00"
blue = "#00f"

[palettes.other]
green = "#0f0"
"##;
        std::fs::write(&starship_path, before).unwrap();

        strip_starship_slate_palette(&starship_path).unwrap();

        let after = std::fs::read_to_string(&starship_path).unwrap();
        assert!(!after.contains("palette = \"slate\""));
        assert!(!after.contains("[palettes.slate]"));
        // Unrelated user palettes stay — only the slate one is removed.
        assert!(after.contains("[palettes.other]"));
        assert!(after.contains("green = \"#0f0\""));
        // User format setting untouched.
        assert!(after.contains("format = \"$all\""));
    }

    #[test]
    fn starship_clean_uses_the_apply_and_snapshot_path() {
        let td = TempDir::new().unwrap();
        let env = SlateEnv::with_home(td.path().to_path_buf());
        let path = crate::adapter::StarshipAdapter::integration_config_path_with_env(&env);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "palette = \"slate\"\n").unwrap();
        remove_starship_managed_references(&env).unwrap();
        assert!(!fs::read_to_string(path).unwrap().contains("slate"));
    }

    #[test]
    fn strip_starship_slate_palette_leaves_user_palette_alone() {
        let td = TempDir::new().unwrap();
        let starship_path = td.path().join("starship.toml");

        // User set palette to their own choice BEFORE slate ever ran.
        // slate never changed palette to "slate" here, so clean must not
        // rip out the user's setting.
        let before = "palette = \"nord\"\n";
        std::fs::write(&starship_path, before).unwrap();

        strip_starship_slate_palette(&starship_path).unwrap();

        let after = std::fs::read_to_string(&starship_path).unwrap();
        assert!(after.contains("palette = \"nord\""));
    }

    #[test]
    fn strip_starship_slate_palette_is_noop_on_missing_file() {
        let td = TempDir::new().unwrap();
        let missing = td.path().join("nope.toml");
        assert!(strip_starship_slate_palette(&missing).is_ok());
    }

    #[test]
    fn strip_starship_slate_palette_does_not_fail_on_unparseable_toml() {
        let td = TempDir::new().unwrap();
        let starship_path = td.path().join("starship.toml");
        std::fs::write(&starship_path, "this is not valid = toml [ {").unwrap();

        // Clean is best-effort — unparseable config must not block uninstall.
        assert!(strip_starship_slate_palette(&starship_path).is_ok());
        // Original garbage content preserved.
        let after = std::fs::read_to_string(&starship_path).unwrap();
        assert!(after.contains("this is not valid = toml"));
    }

    #[test]
    fn strip_starship_slate_palette_drops_empty_palettes_table() {
        let td = TempDir::new().unwrap();
        let starship_path = td.path().join("starship.toml");

        // User's starship.toml had no palettes before slate — after slate
        // ran, the [palettes] table exists with only the slate child.
        // Removing that child must also remove the now-empty parent table.
        let before = r##"palette = "slate"

[palettes.slate]
red = "#f00"
"##;
        std::fs::write(&starship_path, before).unwrap();

        strip_starship_slate_palette(&starship_path).unwrap();

        let after = std::fs::read_to_string(&starship_path).unwrap();
        assert!(!after.contains("[palettes.slate]"));
        assert!(!after.contains("[palettes]"));
    }

    #[test]
    fn remove_slate_owned_config_state_preserves_user_tier() {
        let td = TempDir::new().unwrap();
        let env = SlateEnv::with_home(td.path().to_path_buf());
        let config_dir = env.config_dir();
        let user_file = config_dir.join("user/ghostty/local.conf");
        let managed_file = config_dir.join("managed/ghostty/theme.conf");
        std::fs::create_dir_all(user_file.parent().unwrap()).unwrap();
        std::fs::create_dir_all(managed_file.parent().unwrap()).unwrap();
        std::fs::write(&user_file, "font-size = 14\n").unwrap();
        std::fs::write(&managed_file, "background = #000000\n").unwrap();
        std::fs::write(config_dir.join("current"), "catppuccin-mocha\n").unwrap();

        assert!(remove_slate_owned_config_state(&env).unwrap());

        assert!(user_file.exists(), "user tier must survive slate clean");
        assert!(
            !managed_file.exists(),
            "managed tier must be removed by slate clean"
        );
        assert!(
            !config_dir.join("current").exists(),
            "state files must be removed by slate clean"
        );
    }

    #[test]
    fn remove_kitty_managed_references_strips_slate_lines_and_keeps_user_content() {
        let td = TempDir::new().unwrap();
        let env = SlateEnv::with_home(td.path().to_path_buf());
        let integration_path = td.path().join(".config/kitty/kitty.conf");
        std::fs::create_dir_all(integration_path.parent().unwrap()).unwrap();

        let managed_kitty = env.config_dir().join("managed").join("kitty");
        let contents = format!(
            "allow_remote_control socket-only\n\
             listen_on unix:/tmp/kitty-slate\n\
             dynamic_background_opacity yes\n\
             include {}/theme.conf\n\
             include {}/opacity.conf\n\
             font_family FiraCode Nerd Font\n\
             include /home/user/my-own-theme.conf\n",
            managed_kitty.display(),
            managed_kitty.display(),
        );
        std::fs::write(&integration_path, contents).unwrap();

        remove_kitty_managed_references(&env).unwrap();

        let cleaned = std::fs::read_to_string(&integration_path).unwrap();
        // slate-owned include lines removed
        assert!(!cleaned.contains(&format!("include {}/theme.conf", managed_kitty.display())));
        assert!(!cleaned.contains(&format!("include {}/opacity.conf", managed_kitty.display())));
        // slate-owned listen_on removed (carries the kitty-slate socket marker)
        assert!(!cleaned.contains("listen_on unix:/tmp/kitty-slate"));
        // user content preserved
        assert!(cleaned.contains("font_family FiraCode Nerd Font"));
        assert!(cleaned.contains("include /home/user/my-own-theme.conf"));
        // allow_remote_control and dynamic_background_opacity are left alone — they
        // may pre-date slate and don't carry a slate-specific marker.
        assert!(cleaned.contains("allow_remote_control socket-only"));
        assert!(cleaned.contains("dynamic_background_opacity yes"));
    }

    #[test]
    fn remove_kitty_managed_references_is_noop_on_missing_file() {
        let td = TempDir::new().unwrap();
        let env = SlateEnv::with_home(td.path().to_path_buf());
        assert!(remove_kitty_managed_references(&env).is_ok());
        assert!(!td.path().join(".config/kitty").exists());
    }

    #[test]
    fn remove_kitty_managed_references_preserves_non_utf8_bytes() {
        let td = TempDir::new().unwrap();
        let env = SlateEnv::with_home(td.path().to_path_buf());
        let integration_path = td.path().join(".config/kitty/kitty.conf");
        std::fs::create_dir_all(integration_path.parent().unwrap()).unwrap();

        let mut content = vec![0xff, b'\n'];
        content.extend_from_slice(
            format!(
                "include {}/managed/kitty/theme.conf\nfont_family Mono\n",
                env.config_dir().display()
            )
            .as_bytes(),
        );
        std::fs::write(&integration_path, content).unwrap();

        remove_kitty_managed_references(&env).unwrap();

        let cleaned = std::fs::read(&integration_path).unwrap();
        assert!(cleaned.starts_with(&[0xff, b'\n']));
        let cleaned_str = String::from_utf8_lossy(&cleaned);
        assert!(!cleaned_str.contains("managed/kitty/theme.conf"));
        assert!(cleaned_str.contains("font_family Mono"));
    }

    #[test]
    fn remove_ghostty_managed_references_preserves_non_utf8_bytes() {
        let td = TempDir::new().unwrap();
        let env = SlateEnv::with_home(td.path().to_path_buf());
        let integration_path = td.path().join(".config/ghostty/config.ghostty");
        std::fs::create_dir_all(integration_path.parent().unwrap()).unwrap();

        let mut content = vec![0xff, b'\n'];
        content.extend_from_slice(
            format!(
                "config-file = \"{}/managed/ghostty/theme.conf\"\nuser-setting = true\n",
                env.config_dir().display()
            )
            .as_bytes(),
        );
        std::fs::write(&integration_path, content).unwrap();

        remove_ghostty_managed_references(&env).unwrap();

        let cleaned = std::fs::read(&integration_path).unwrap();
        assert!(cleaned.starts_with(&[0xff, b'\n']));
        let cleaned_str = String::from_utf8_lossy(&cleaned);
        assert!(!cleaned_str.contains("config-file ="));
        assert!(cleaned_str.contains("user-setting = true"));
    }

    #[test]
    fn remove_ghostty_managed_references_cleans_all_candidate_configs() {
        let td = TempDir::new().unwrap();
        let env = SlateEnv::with_home(td.path().to_path_buf());
        let ghostty_dir = td.path().join(".config/ghostty");
        std::fs::create_dir_all(&ghostty_dir).unwrap();

        for file_name in ["config.ghostty", "config"] {
            std::fs::write(
                ghostty_dir.join(file_name),
                format!(
                    "config-file = \"{}/managed/ghostty/theme.conf\"\nuser-setting = true\n",
                    env.config_dir().display()
                ),
            )
            .unwrap();
        }

        remove_ghostty_managed_references(&env).unwrap();

        for file_name in ["config.ghostty", "config"] {
            let cleaned = std::fs::read_to_string(ghostty_dir.join(file_name)).unwrap();
            assert!(!cleaned.contains("config-file ="));
            assert!(cleaned.contains("user-setting = true"));
        }
    }
}
