use crate::adapter::{GhosttyAdapter, ToolAdapter};
use crate::brand::events::{dispatch, BrandEvent, FailureKind, SuccessKind};
use crate::brand::render_context::RenderContext;
use crate::brand::roles::Roles;
use crate::env::SlateEnv;
use crate::error::Result;
use crate::{config::ConfigManager, platform};
use std::fs;
use std::path::Path;

/// Handle `slate clean` command
/// Removes managed files, stops the auto-theme watcher, and removes.zshrc marker block
/// Clean removes slate-managed assets; see 'slate restore' to recover from snapshot
pub fn handle_clean() -> Result<()> {
    match handle_clean_inner() {
        Ok(()) => Ok(()),
        Err(err) => {
            // D-17: clean-level failure → `BrandEvent::Failure(CleanFailed)`
            // so Phase 20's SoundSink maps the error moment to the failure
            // SFX. Paired Success events dispatch from the happy path below.
            dispatch(BrandEvent::Failure(FailureKind::CleanFailed));
            Err(err)
        }
    }
}

fn handle_clean_inner() -> Result<()> {
    use cliclack::{intro, log};

    // Build a RenderContext up-front so every user-visible status line
    // shares the same byte contract (sketch 003 tree narrative + D-01
    // daily chrome + D-01a severity). D-05 graceful degrade — plain text
    // when the theme registry fails to load.
    let ctx = RenderContext::from_active_theme().ok();
    let r = ctx.as_ref().map(Roles::new);

    intro(intro_title(r.as_ref(), "Clean Up Slate"))?;

    let env = SlateEnv::from_process()?;
    let config = ConfigManager::with_env(&env)?;

    let mut removed_sections: Vec<&'static str> = Vec::new();

    // Step 0: Snapshot the current state so the user can undo this clean. Without this
    // the only restore point after clean is the pre-slate baseline, which is the wrong
    // target if the user just wants to roll back the clean itself. Best-effort — we don't
    // want a backup hiccup to block the clean.
    {
        let label = config
            .get_current_theme()
            .ok()
            .flatten()
            .map(|theme| format!("pre-clean-{}", theme))
            .unwrap_or_else(|| "pre-clean".to_string());
        if let Err(err) = crate::config::snapshot_current_state_with_env(&env, &label) {
            log::remark(format!("  (couldn't create pre-clean snapshot: {})", err))?;
        } else {
            log::success(status_success_line(
                r.as_ref(),
                &format!("Saved pre-clean snapshot ({})", label),
            ))?;
        }
    }

    // Step 1: Stop watcher + clear config flag
    log::step("Stopping auto-theme watcher...")?;
    if let Err(err) = config.set_auto_theme_enabled(false) {
        log::remark(format!("  (couldn't update auto-theme flag: {})", err))?;
    }
    platform::dark_mode_notify::stop()?;
    platform::dark_mode_notify::remove_binary(&config)?;
    log::success(status_success_line(r.as_ref(), "Watcher stopped"))?;
    removed_sections.push("auto-theme watcher");

    // Step 2: Remove integration references before deleting managed files
    log::step("Removing integration references...")?;
    remove_marker_block_from_zshrc(env.home())?;
    remove_marker_blocks_from_bash(&env)?;
    remove_fish_loader(&env)?;
    remove_ghostty_managed_references(&env)?;
    remove_alacritty_managed_references(&env)?;
    remove_tmux_managed_references(env.home())?;
    remove_delta_managed_references(env.home())?;
    remove_nvim_managed_references(&env)?;
    log::success(status_success_line(
        r.as_ref(),
        "Removed config-file/import/source hooks",
    ))?;
    removed_sections.push("shell + tool hooks");

    // Step 3: Delete Slate-owned config directory
    log::step("Removing Slate-managed config state...")?;
    let config_dir = env.config_dir();
    if config_dir.exists() {
        fs::remove_dir_all(config_dir)?;
        log::success(status_success_line(r.as_ref(), "Removed ~/.config/slate"))?;
        removed_sections.push("managed config state");
    } else {
        log::remark("  (~/.config/slate already removed)")?;
    }

    // Step 4: Reload running terminals so the theme actually drops.
    // Removing the config-file line from ~/.config/ghostty/config only takes effect on the
    // next reload; without this, users see "clean succeeded" but the background + palette
    // stay applied until they restart Ghostty themselves. Best-effort — if the terminal
    // isn't running we silently move on.
    let _ = GhosttyAdapter.reload();

    // D-10: completion receipt is a static tree-narrative anchor — bypass
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
    log::info(
        "clean removed Slate-owned shell hooks, watcher artifacts, and config state. \
Third-party tools installed through Homebrew remain installed. \
Use 'slate restore' before cleanup if you want to roll back to a snapshot instead.",
    )?;

    // D-17: clean success → paired `CleanComplete` (category) +
    // `ApplyComplete` (whole-flow milestone) so Phase 20 can latch onto
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
/// — NEVER lavender per D-01a), falling back to plain `✓ message`.
fn status_success_line(r: Option<&Roles<'_>>, message: &str) -> String {
    match r {
        Some(r) => r.status_success(message),
        None => format!("✓ {}", message),
    }
}

/// Render `◆ title` via `Roles::heading`, falling back to plain ◆ text
/// when Roles is unavailable (D-05 graceful degrade).
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

/// Remove marker block from.zshrc
/// Handles multiple blocks and preserves rest of file content
fn remove_marker_block_from_zshrc(home: &Path) -> Result<()> {
    let zshrc_path = home.join(".zshrc");
    crate::adapter::marker_block::remove_managed_blocks_from_file(&zshrc_path)
}

/// Remove marker blocks from any bash rc file Slate might have written to.
///
/// On macOS we may have written to `.bash_profile` (login-shell convention); on Linux we
/// write to `.bashrc`. Sweep both so a reinstall/clean across machines or a migration from
/// an older slate version still leaves no orphaned loaders. `remove_managed_blocks_from_file`
/// is a no-op on missing files, so unconditional calls are safe.
fn remove_marker_blocks_from_bash(env: &SlateEnv) -> Result<()> {
    crate::adapter::marker_block::remove_managed_blocks_from_file(&env.bashrc_path())?;
    crate::adapter::marker_block::remove_managed_blocks_from_file(&env.bash_profile_path())?;
    Ok(())
}

fn remove_fish_loader(env: &SlateEnv) -> Result<()> {
    match fs::remove_file(env.fish_loader_path()) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err.into()),
    }
}

fn remove_ghostty_managed_references(env: &SlateEnv) -> Result<()> {
    let adapter = crate::adapter::GhosttyAdapter;
    let integration_path = adapter.integration_config_path_with_env(env)?;
    if !integration_path.exists() {
        return Ok(());
    }

    let managed_prefix = env
        .config_dir()
        .join("managed")
        .join("ghostty")
        .to_string_lossy()
        .to_string();
    let content = fs::read(&integration_path)?;
    let mut cleaned: Vec<u8> = Vec::with_capacity(content.len());

    for line in content.split_inclusive(|b| *b == b'\n') {
        let trimmed = line
            .iter()
            .copied()
            .skip_while(|b| b.is_ascii_whitespace())
            .collect::<Vec<u8>>();
        if trimmed.starts_with(b"config-file")
            && trimmed
                .windows(managed_prefix.len())
                .any(|w| w == managed_prefix.as_bytes())
        {
            continue;
        }
        cleaned.extend_from_slice(line);
    }

    fs::write(integration_path, cleaned)?;
    Ok(())
}

fn remove_alacritty_managed_references(env: &SlateEnv) -> Result<()> {
    let integration_path =
        crate::adapter::alacritty::AlacrittyAdapter::integration_config_path_with_env(env);
    if !integration_path.exists() {
        return Ok(());
    }

    let managed_prefix = env
        .config_dir()
        .join("managed")
        .join("alacritty")
        .to_string_lossy()
        .to_string();
    let content = match fs::read(&integration_path) {
        Ok(bytes) => match String::from_utf8(bytes) {
            Ok(content) => content,
            Err(_) => return Ok(()),
        },
        Err(err) => return Err(err.into()),
    };
    let mut doc: toml_edit::DocumentMut = if content.trim().is_empty() {
        toml_edit::DocumentMut::new()
    } else {
        content.parse().map_err(|e| {
            crate::error::SlateError::InvalidConfig(format!(
                "Failed to parse Alacritty TOML during clean: {}",
                e
            ))
        })?
    };

    if let Some(imports) = doc
        .get_mut("general")
        .and_then(|general| general.get_mut("import"))
        .and_then(|imports| imports.as_array_mut())
    {
        let retained: Vec<String> = imports
            .iter()
            .filter_map(|item| item.as_str())
            .filter(|path| !path.contains(&managed_prefix))
            .map(ToString::to_string)
            .collect();

        imports.clear();
        for path in retained {
            imports.push(path);
        }
    }

    if doc
        .get("general")
        .and_then(|general| general.as_table())
        .and_then(|table| table.get("import"))
        .and_then(|imports| imports.as_array())
        .is_some_and(|imports| imports.is_empty())
    {
        if let Some(general) = doc.get_mut("general").and_then(|item| item.as_table_mut()) {
            general.remove("import");
        }
    }

    if doc
        .get("general")
        .and_then(|general| general.as_table())
        .is_some_and(|table| table.is_empty())
    {
        doc.remove("general");
    }

    fs::write(integration_path, doc.to_string())?;
    Ok(())
}

fn remove_tmux_managed_references(home: &Path) -> Result<()> {
    let tmux_path = home.join(".tmux.conf");
    crate::adapter::marker_block::remove_managed_blocks_from_file(&tmux_path)
}

fn remove_delta_managed_references(home: &Path) -> Result<()> {
    let gitconfig_path = home.join(".gitconfig");
    crate::adapter::marker_block::remove_managed_blocks_from_file(&gitconfig_path)
}

/// Remove every slate-owned file under `~/.config/nvim/` plus the
/// state file in `~/.cache/slate/`, and best-effort strip the
/// `pcall(require, 'slate')` marker block from init.lua / init.vim
/// (Phase 17 D-03). Non-slate files in `colors/` are preserved.
///
/// Each step is best-effort — a missing file or directory is NOT
/// an error (mirrors `remove_fish_loader`'s posture). The orphan
/// safety of `pcall(require, 'slate')` means failure on the
/// marker-block strip is cosmetic only: nvim startup still
/// succeeds because `pcall` swallows the missing-module error.
fn remove_nvim_managed_references(env: &SlateEnv) -> Result<()> {
    let nvim_home = env.home().join(".config/nvim");

    // 1. Remove every `slate-*.lua` shim under ~/.config/nvim/colors/.
    //    User-owned files (my-custom.lua, theme.lua, …) are preserved
    //    — Pitfall 7 guard verified by
    //    `remove_nvim_managed_references_leaves_user_files_alone`.
    let colors_dir = nvim_home.join("colors");
    if colors_dir.exists() {
        for entry in fs::read_dir(&colors_dir)? {
            let entry = entry?;
            let name = entry.file_name();
            if name.to_string_lossy().starts_with("slate-") {
                let _ = fs::remove_file(entry.path());
            }
        }
    }

    // 2. Remove the loader dir ~/.config/nvim/lua/slate/ (slate-owned).
    let loader_dir = nvim_home.join("lua").join("slate");
    if loader_dir.exists() {
        let _ = fs::remove_dir_all(&loader_dir);
    }

    // 3. Best-effort strip the D-09 marker block from init.lua / init.vim.
    //    Primitive is a no-op on missing files, so both calls are safe
    //    unconditionally. Errors are swallowed so a corrupted init file
    //    on one path doesn't abort the clean of the other.
    let _ =
        crate::adapter::marker_block::remove_managed_blocks_from_file(&nvim_home.join("init.lua"));
    let _ =
        crate::adapter::marker_block::remove_managed_blocks_from_file(&nvim_home.join("init.vim"));

    // 4. Remove the state file ~/.cache/slate/current_theme.lua.
    //    `Step 3: Remove Slate-managed config state` in handle_clean
    //    deletes the whole ~/.config/slate/ tree but the nvim state
    //    file lives under ~/.cache/slate/, so the explicit removal
    //    here guarantees no orphan state file survives.
    let state_file = env.slate_cache_dir().join("current_theme.lua");
    if state_file.exists() {
        let _ = fs::remove_file(&state_file);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::brand::render_context::{mock_context_with_mode, mock_theme, RenderMode};
    use crate::theme::ThemeRegistry;
    use tempfile::TempDir;

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

    /// Wave 4 snapshot — byte-lock the `slate clean` completion tree in
    /// Basic mode so the sketch-003 tree narrative stays stable across
    /// CI and contributor workstations (D-06 MockTheme).
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

    /// D-05 graceful degrade — without Roles the tree falls back to
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
    fn remove_ghostty_managed_references_preserves_non_utf8_bytes() {
        let td = TempDir::new().unwrap();
        let env = SlateEnv::with_home(td.path().to_path_buf());
        let integration_path = td.path().join(".config/ghostty/config");
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
}
