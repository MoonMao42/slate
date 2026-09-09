//! Potential configuration writes, shared by theme and import checkpoints.
//! No executable probes, directory creation or recursive scans happen here.
use super::{
    AlacrittyAdapter, BatAdapter, GhosttyAdapter, KittyAdapter, OpencodeAdapter, StarshipAdapter,
};
use crate::{
    env::SlateEnv,
    error::{Result, SlateError},
    theme::ThemeRegistry,
};
use std::{collections::BTreeSet, path::PathBuf};

pub(crate) fn shared_theme_paths(env: &SlateEnv) -> BTreeSet<PathBuf> {
    let mut files: BTreeSet<_> = [
        "current",
        "auto.toml",
        "managed/shell/env.zsh",
        "managed/shell/env.bash",
        "managed/shell/env.fish",
        "managed/starship/plain.toml",
    ]
    .into_iter()
    .map(|name| env.managed_file(name))
    .collect();
    files.insert(env.slate_cache_dir().join("current_theme.lua"));
    files
}

pub(crate) fn add_theme_paths(
    env: &SlateEnv,
    tool: &str,
    files: &mut BTreeSet<PathBuf>,
) -> Result<()> {
    match tool {
        // Ghostty theme apply can also refresh a saved font; the other two
        // terminal theme writers do not write font files.
        "ghostty" | "alacritty" | "kitty" => terminal_paths(env, tool, tool == "ghostty", files)?,
        "starship" => {
            files.insert(StarshipAdapter::integration_config_path_with_env(env));
        }
        "bat" => {
            env.validate_bat_paths()?;
            let directory = BatAdapter.themes_dir(env);
            for theme in ThemeRegistry::new()?.all() {
                files.insert(directory.join(format!("slate-{}.tmTheme", theme.id)));
            }
        }
        "btop" => {
            files.insert(super::BtopAdapter::config_path(env));
            files.insert(super::BtopAdapter::theme_path(env));
        }
        "yazi" => files.extend(super::YaziAdapter::paths(env)),
        "zellij" => files.extend(super::ZellijAdapter::paths(env)?),
        "delta" => {
            files.insert(env.managed_file("managed/delta/colors"));
            files.insert(env.home().join(".gitconfig"));
        }
        "tmux" => {
            files.insert(env.managed_file("managed/tmux/colors.conf"));
            files.extend(env.tmux_config_candidates());
        }
        "eza" => {
            files.insert(super::EzaAdapter::theme_path(env));
        }
        "lazygit" => {
            files.insert(super::LazygitAdapter::theme_path(env));
        }
        "fastfetch" => {
            files.insert(super::FastfetchAdapter::theme_path(env));
        }
        "zsh-syntax-highlighting" => {
            files.insert(super::ZshHighlightAdapter::theme_path(env));
        }
        "opencode" => {
            env.validate_opencode_tui_config()?;
            files.extend(OpencodeAdapter::tui_config_paths(env));
        }
        // The notification is shared by global theme apply, but a selected-only
        // sync must also capture it without opting into other shared writes.
        "nvim" => {
            files.insert(env.slate_cache_dir().join("current_theme.lua"));
        }
        "ls_colors" => {}
        _ => {
            return Err(SlateError::InvalidConfig(format!(
                "No theme recovery path contract for {}; no theme files were written",
                tool.escape_default(),
            )))
        }
    }
    Ok(())
}

pub(crate) fn terminal_paths(
    env: &SlateEnv,
    tool: &str,
    include_font: bool,
    files: &mut BTreeSet<PathBuf>,
) -> Result<()> {
    let names: &[&str] = match tool {
        "ghostty" => {
            files.extend(GhosttyAdapter.integration_candidate_paths_with_env(env)?);
            &["theme.conf", "opacity.conf", "blur.conf"]
        }
        "alacritty" => {
            files.insert(AlacrittyAdapter::integration_config_path_with_env(env));
            &["colors.toml", "opacity.toml"]
        }
        "kitty" => {
            files.insert(KittyAdapter::resolve_config_path_with_env(env));
            &["theme.conf", "opacity.conf"]
        }
        _ => {
            return Err(SlateError::InvalidConfig(
                "Unknown terminal recovery contract".into(),
            ))
        }
    };
    let managed = env.config_dir().join("managed").join(tool);
    files.extend(names.iter().map(|name| managed.join(name)));
    if include_font {
        files.insert(managed.join(if tool == "alacritty" {
            "font.toml"
        } else {
            "font.conf"
        }));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapter::{ApplyStrategy, ToolRegistry};

    #[test]
    fn theme_write_paths_cover_all_registered_theme_adapters_without_native_probes() {
        let td = tempfile::tempdir().unwrap();
        let env = SlateEnv::with_home(td.path().to_owned());
        let mut files = shared_theme_paths(&env);
        for adapter in ToolRegistry::default().adapters() {
            if adapter.apply_strategy() != ApplyStrategy::DetectAndInstall {
                add_theme_paths(&env, adapter.tool_name(), &mut files).unwrap();
            }
        }
        assert!(files
            .iter()
            .all(|path| path.is_absolute() && path.starts_with(td.path())));
        for name in [
            "managed/ghostty/theme.conf",
            "managed/ghostty/font.conf",
            "managed/ghostty/blur.conf",
            "managed/alacritty/colors.toml",
            "managed/kitty/theme.conf",
            "managed/delta/colors",
            "managed/eza/theme.yml",
            "managed/lazygit/config.yml",
            "managed/fastfetch/config.jsonc",
            "managed/tmux/colors.conf",
            "managed/zsh/highlight-styles.sh",
            "managed/starship/plain.toml",
        ] {
            assert!(files.contains(&env.managed_file(name)), "{name}");
        }
        assert!(files.contains(&env.bat_config_dir().join("themes/slate-nord.tmTheme")));
        assert!(!files.contains(&env.bat_config_path().to_owned()));
        assert!(!files.contains(&env.nvim_init_path()));
        assert!(!files.contains(&env.zshrc_path()));
        assert!(!files.contains(&env.managed_file("managed/kitty/font.conf")));
        assert!(add_theme_paths(&env, "unknown-adapter", &mut files).is_err());
        assert_eq!(std::fs::read_dir(td.path()).unwrap().count(), 0);
    }

    #[test]
    fn theme_write_paths_selection_excludes_unrelated_outputs_but_font_import_can_extend_it() {
        let td = tempfile::tempdir().unwrap();
        let env = SlateEnv::with_home(td.path().to_owned());
        let mut files = shared_theme_paths(&env);
        add_theme_paths(&env, "alacritty", &mut files).unwrap();
        assert!(!files.contains(&env.home().join(".gitconfig")));
        assert!(!files.contains(&env.managed_file("managed/alacritty/font.toml")));
        assert!(!files.iter().any(|p| p.starts_with(env.bat_config_dir())));
        terminal_paths(&env, "alacritty", true, &mut files).unwrap();
        assert!(files.contains(&env.managed_file("managed/alacritty/font.toml")));
        assert_eq!(std::fs::read_dir(td.path()).unwrap().count(), 0);
    }
}
