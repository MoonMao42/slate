use super::manifest::{
    append_manifest_entry, write_manifest_raw, RestoreManifest, RestoreManifestMetadata,
};
use super::restore::get_restore_point_with_env;
use super::{
    backup_directory_with_env, manifest_path, resolve_restore_point_directory, BackupSession,
    OriginalFileState, RestoreEntry, RestorePoint,
};
use crate::config::file_read::Links;
use crate::config::state_files::atomic_write_synced_mode;
use crate::env::SlateEnv;
use crate::error::{Result, SlateError};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

struct SnapshotTarget {
    tool_key: &'static str,
    display_tool: &'static str,
    path: PathBuf,
}

enum SnapshotTargets<'a> {
    Standard,
    Exact(&'a [RestoreEntry]),
    Strict(&'a [RestoreEntry]),
}

pub fn create_backup_with_session(
    tool_key: &str,
    display_tool: &str,
    session: &BackupSession,
    config_path: &Path,
) -> Result<RestoreEntry> {
    let mut remaining = super::source::MAX_SNAPSHOT_BYTES;
    let source = super::source::read_with_links(config_path, &mut remaining, Links::Follow)?
        .ok_or_else(|| SlateError::ConfigNotFound(config_path.display().to_string()))?;

    let entry = write_backup_file(
        tool_key,
        display_tool,
        session,
        config_path,
        &source.bytes,
        source.mode,
    )?;
    append_manifest_entry(session, &entry)?;
    Ok(entry)
}

fn write_backup_file(
    tool_key: &str,
    display_tool: &str,
    session: &BackupSession,
    config_path: &Path,
    content: &[u8],
    unix_mode: Option<u32>,
) -> Result<RestoreEntry> {
    super::validate_restore_point_id(tool_key)?;
    let backup_filename = format!("{}.backup", tool_key);
    let backup_path = session.restore_point_dir.join(&backup_filename);

    atomic_write_synced_mode(&backup_path, content, Some(0o600))
        .map_err(|e| SlateError::BackupFailed(format!("Failed to write backup file: {}", e)))?;

    let restore_entry = RestoreEntry {
        tool_key: tool_key.to_string(),
        display_tool: display_tool.to_string(),
        original_path: config_path.to_path_buf(),
        backup_path: Some(backup_path),
        original_state: OriginalFileState::Present,
        unix_mode,
    };

    Ok(restore_entry)
}

fn baseline_snapshot_targets(env: &SlateEnv) -> Result<Vec<SnapshotTarget>> {
    let kitty_path = env.xdg_config_home().join("kitty/kitty.conf");
    // Snapshot the same user config the adapter edits, not STARSHIP_CONFIG,
    // which can point at a generated fallback or another process's config.
    let starship_path = crate::adapter::StarshipAdapter::integration_config_path_with_env(env);
    let nvim_init_lua = env.nvim_config_dir().join("init.lua");
    let nvim_init_vim = env.nvim_config_dir().join("init.vim");

    let mut targets = vec![
        SnapshotTarget {
            tool_key: "zshrc",
            display_tool: "Zsh",
            path: env.zshrc_path(),
        },
        SnapshotTarget {
            tool_key: "bashrc",
            display_tool: "Bash",
            path: env.bashrc_path(),
        },
        // Capture all supported login entries, so restore includes whichever
        // entry was selected without masking an existing lower-priority file.
        SnapshotTarget {
            tool_key: "bash-profile",
            display_tool: "Bash (login profile)",
            path: env.bash_profile_path(),
        },
        SnapshotTarget {
            tool_key: "bash-login",
            display_tool: "Bash (login)",
            path: env.bash_login_path(),
        },
        SnapshotTarget {
            tool_key: "shell-profile",
            display_tool: "Shell (shared profile)",
            path: env.shell_profile_path(),
        },
        SnapshotTarget {
            tool_key: "fish-loader",
            display_tool: "Fish",
            path: env.fish_loader_path(),
        },
        SnapshotTarget {
            tool_key: "gitconfig",
            display_tool: "Git",
            path: env.home().join(".gitconfig"),
        },
        SnapshotTarget {
            tool_key: "tmux",
            display_tool: "tmux",
            path: env.tmux_config_path(),
        },
    ];

    targets.extend(ghostty_snapshot_targets(env));
    // Preserve every user candidate, including absence. Setup, alternate
    // entries and a later change in precedence must remain recoverable.
    const ALACRITTY_KEYS: [&str; 4] = [
        "alacritty",
        "alacritty-alternate-1",
        "alacritty-alternate-2",
        "alacritty-alternate-3",
    ];
    targets.extend(
        ALACRITTY_KEYS
            .into_iter()
            .zip(crate::adapter::AlacrittyAdapter::integration_candidate_paths_with_env(env))
            .map(|(tool_key, path)| SnapshotTarget {
                tool_key,
                display_tool: "Alacritty",
                path,
            }),
    );

    targets.extend([
        SnapshotTarget {
            tool_key: "kitty",
            display_tool: "Kitty",
            path: kitty_path,
        },
        SnapshotTarget {
            tool_key: "starship",
            display_tool: "Starship",
            path: starship_path,
        },
        SnapshotTarget {
            tool_key: "btop",
            display_tool: "btop configuration",
            path: crate::adapter::BtopAdapter::config_path(env),
        },
        SnapshotTarget {
            tool_key: "btop-theme",
            display_tool: "Slate btop theme",
            path: crate::adapter::BtopAdapter::theme_path(env),
        },
        SnapshotTarget {
            tool_key: "yazi",
            display_tool: "Yazi theme selection",
            path: crate::adapter::YaziAdapter::config_path(env),
        },
        SnapshotTarget {
            tool_key: "yazi-flavor",
            display_tool: "Slate Yazi flavor",
            path: crate::adapter::YaziAdapter::flavor_path(env),
        },
        SnapshotTarget {
            tool_key: "yazi-syntax",
            display_tool: "Slate Yazi syntax colors",
            path: crate::adapter::YaziAdapter::syntax_path(env),
        },
        SnapshotTarget {
            tool_key: "opencode-tui",
            display_tool: "OpenCode",
            path: crate::adapter::OpencodeAdapter::tui_config_path(env),
        },
        // nvim init files get a slate marker block (pcall(require, 'slate'))
        // on setup; baseline must capture the pre-install state so restore
        // can remove the block. Both .lua and .vim are included because
        // slate writes to whichever exists.
        SnapshotTarget {
            tool_key: "nvim-init-lua",
            display_tool: "Neovim (init.lua)",
            path: nvim_init_lua,
        },
        SnapshotTarget {
            tool_key: "nvim-init-vim",
            display_tool: "Neovim (init.vim)",
            path: nvim_init_vim,
        },
        SnapshotTarget {
            tool_key: "nvim-auto-activation",
            display_tool: "Neovim auto-activation preference",
            path: env.nvim_auto_activation_path(),
        },
        SnapshotTarget {
            tool_key: "slate-current",
            display_tool: "Slate current theme",
            path: env.managed_file("current"),
        },
        SnapshotTarget {
            tool_key: "slate-current-font",
            display_tool: "Slate current font",
            path: env.managed_file("current-font"),
        },
        SnapshotTarget {
            tool_key: "slate-current-opacity",
            display_tool: "Slate current opacity",
            path: env.managed_file("current-opacity"),
        },
        SnapshotTarget {
            tool_key: "slate-config",
            display_tool: "Slate config",
            path: env.managed_file("config.toml"),
        },
        SnapshotTarget {
            tool_key: "slate-auto",
            display_tool: "Slate auto theme",
            path: env.managed_file("auto.toml"),
        },
        SnapshotTarget {
            tool_key: "slate-fastfetch",
            display_tool: "Slate fastfetch autorun",
            path: env.managed_file("autorun-fastfetch"),
        },
        SnapshotTarget {
            tool_key: "slate-auto-watcher",
            display_tool: "Slate auto-theme watcher",
            path: env.config_dir().join("managed/bin/slate-dark-mode-notify"),
        },
        SnapshotTarget {
            tool_key: "slate-appearance-helper",
            display_tool: "Slate appearance event helper",
            path: env.config_dir().join("managed/bin/slate-appearance-helper"),
        },
        SnapshotTarget {
            tool_key: "slate-shell-zsh",
            display_tool: "Slate shell env (zsh)",
            path: env.config_dir().join("managed/shell/env.zsh"),
        },
        SnapshotTarget {
            tool_key: "slate-shell-bash",
            display_tool: "Slate shell env (bash)",
            path: env.config_dir().join("managed/shell/env.bash"),
        },
        SnapshotTarget {
            tool_key: "slate-shell-fish",
            display_tool: "Slate shell env (fish)",
            path: env.config_dir().join("managed/shell/env.fish"),
        },
    ]);

    targets.extend(
        crate::opacity::MANAGED_FILES
            .into_iter()
            .map(|file| SnapshotTarget {
                tool_key: file.key(),
                display_tool: "Terminal opacity",
                path: file.path(env),
            }),
    );
    let [config, theme] = crate::adapter::ZellijAdapter::paths(env)?;
    targets.extend([
        SnapshotTarget {
            tool_key: "zellij",
            display_tool: "Zellij configuration",
            path: config,
        },
        SnapshotTarget {
            tool_key: "zellij-theme",
            display_tool: "Slate Zellij theme",
            path: theme,
        },
    ]);
    Ok(targets)
}

fn ghostty_snapshot_targets(env: &SlateEnv) -> Vec<SnapshotTarget> {
    crate::adapter::GhosttyAdapter::config_candidates_with_env(env)
        .unwrap_or_else(|_| crate::adapter::GhosttyAdapter::xdg_config_candidates(env))
        .into_iter()
        .map(|candidate| SnapshotTarget {
            tool_key: candidate.key,
            display_tool: "Ghostty",
            path: candidate.path,
        })
        .collect()
}

fn write_restore_manifest(
    session: &BackupSession,
    created_at: SystemTime,
    is_baseline: bool,
    entries: Vec<RestoreEntry>,
) -> Result<()> {
    let manifest = RestoreManifest {
        metadata: RestoreManifestMetadata {
            id: session.restore_point_id.clone(),
            theme_name: session.theme_name.clone(),
            created_at: super::format_iso8601_timestamp(created_at),
            is_baseline,
        },
        entries,
    };
    write_manifest_raw(&manifest_path(&session.restore_point_dir), &manifest)
}

fn create_snapshot_with_policy(
    env: &SlateEnv,
    theme_name: String,
    is_baseline: bool,
    record_absent_entries: bool,
    create_error: &str,
    allocation_error: &str,
    restore_targets: SnapshotTargets<'_>,
) -> Result<RestorePoint> {
    if matches!(&restore_targets, SnapshotTargets::Standard) {
        env.validate_opencode_tui_config()?;
    }
    let created_at = SystemTime::now();
    let backup_dir = backup_directory_with_env(env)?;

    for _ in 0..32 {
        let restore_point_id = super::generate_restore_point_id(created_at);
        let restore_point_dir = resolve_restore_point_directory(&backup_dir, &restore_point_id)?;

        let mut directory = fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            directory.mode(0o700);
        }
        match directory.create(&restore_point_dir) {
            Ok(()) => {
                let session = BackupSession {
                    restore_point_id,
                    theme_name: theme_name.clone(),
                    restore_point_dir,
                };
                let snapshot_result = (|| -> Result<RestorePoint> {
                    let strict = matches!(restore_targets, SnapshotTargets::Strict(_));

                    // Undo must cover the files this restore will touch, including old
                    // XDG roots and profiles that differ from the current environment.
                    let targets: Vec<_> = match restore_targets {
                        SnapshotTargets::Exact(entries) | SnapshotTargets::Strict(entries) => {
                            entries
                                .iter()
                                .map(|entry| {
                                    (
                                        entry.tool_key.clone(),
                                        entry.display_tool.clone(),
                                        entry.original_path.clone(),
                                    )
                                })
                                .collect()
                        }
                        SnapshotTargets::Standard => baseline_snapshot_targets(env)?
                            .into_iter()
                            .map(|target| {
                                (
                                    target.tool_key.to_string(),
                                    target.display_tool.to_string(),
                                    target.path,
                                )
                            })
                            .collect(),
                    };
                    let mut remaining = super::source::MAX_SNAPSHOT_BYTES;
                    let mut captured = Vec::new();
                    for (tool_key, display_tool, path) in targets {
                        let source = if strict {
                            super::source::read(&path, &mut remaining)?
                        } else {
                            super::source::read_with_links(&path, &mut remaining, Links::Follow)?
                        };
                        if let Some(source) = source {
                            captured.push(write_backup_file(
                                &tool_key,
                                &display_tool,
                                &session,
                                &path,
                                &source.bytes,
                                source.mode,
                            )?);
                            continue;
                        }
                        if record_absent_entries {
                            let absent_entry = RestoreEntry {
                                tool_key,
                                display_tool,
                                original_path: path,
                                backup_path: None,
                                original_state: OriginalFileState::Absent,
                                unix_mode: None,
                            };
                            captured.push(absent_entry);
                        }
                    }
                    // Publish only complete snapshots, including baseline and
                    // pre-restore captures. Interrupted copies have no manifest.
                    write_restore_manifest(&session, created_at, is_baseline, captured)?;

                    get_restore_point_with_env(env, &session.restore_point_id)
                })();
                if let Err(err) = snapshot_result {
                    // This directory was allocated by this attempt. A partially
                    // populated manifest must never look like a usable snapshot.
                    if let Err(cleanup) = fs::remove_dir_all(&session.restore_point_dir) {
                        return Err(SlateError::BackupFailed(format!(
                            "{err}; cannot remove incomplete snapshot {}: {cleanup}",
                            session.restore_point_dir.display(),
                        )));
                    }
                    return Err(err);
                }
                return snapshot_result;
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => {
                return Err(SlateError::BackupFailed(format!("{}: {}", create_error, e)));
            }
        }
    }

    Err(SlateError::BackupFailed(allocation_error.to_string()))
}

pub fn begin_restore_point_baseline(home: &Path) -> Result<RestorePoint> {
    let env = SlateEnv::with_home(home.to_path_buf());
    begin_restore_point_baseline_with_env(&env)
}

pub fn begin_restore_point_baseline_with_env(env: &SlateEnv) -> Result<RestorePoint> {
    create_snapshot_with_policy(
        env,
        "baseline-pre-slate".to_string(),
        true,
        true,
        "Failed to create baseline restore point directory",
        "Failed to allocate a unique baseline restore point ID",
        SnapshotTargets::Standard,
    )
}

pub fn snapshot_current_state(theme_name: &str) -> Result<RestorePoint> {
    let env = SlateEnv::from_process()
        .map_err(|_| SlateError::Internal("Cannot initialize SlateEnv for snapshot".to_string()))?;
    snapshot_current_state_with_env(&env, theme_name)
}

pub fn snapshot_current_state_with_env(env: &SlateEnv, theme_name: &str) -> Result<RestorePoint> {
    create_snapshot_with_policy(
        env,
        theme_name.to_string(),
        false,
        true,
        "Failed to create snapshot directory",
        "Failed to allocate a unique snapshot ID",
        SnapshotTargets::Standard,
    )
}

/// Clean deletes generated assets as well as integration hooks, so the ordinary
/// theme snapshot is insufficient. Targets come from clean's full preflight.
pub(crate) fn snapshot_clean_targets_with_env(
    env: &SlateEnv,
    label: &str,
    targets: &[RestoreEntry],
) -> Result<RestorePoint> {
    create_snapshot_with_policy(
        env,
        label.to_owned(),
        false,
        true,
        "Failed to create pre-clean snapshot directory",
        "Failed to allocate a unique pre-clean snapshot ID",
        SnapshotTargets::Exact(targets),
    )
}

/// Import supplies its exact potential writes, including files not yet present.
/// Read bounded regular files without following a final symlink or blocking on a
/// FIFO; any failure removes this attempt's incomplete snapshot before returning.
pub(crate) fn snapshot_import_targets_with_env(
    env: &SlateEnv,
    targets: &[RestoreEntry],
) -> Result<RestorePoint> {
    create_snapshot_with_policy(
        env,
        "pre-import".into(),
        false,
        true,
        "Failed to create pre-import snapshot directory",
        "Failed to allocate a unique pre-import snapshot ID",
        SnapshotTargets::Strict(targets),
    )
}

/// Configuration operations supply their exact potential file writes, including
/// absent files. File recovery does not restore watcher processes.
pub(crate) fn snapshot_config_targets_with_env(
    env: &SlateEnv,
    targets: &[RestoreEntry],
) -> Result<RestorePoint> {
    create_snapshot_with_policy(
        env,
        "pre-config".into(),
        false,
        true,
        "Failed to create pre-config snapshot directory",
        "Failed to allocate a unique pre-config snapshot ID",
        SnapshotTargets::Strict(targets),
    )
}

/// Ordinary theme changes preserve only their potential writes, including
/// generated files and absence. These are not global pre-install baselines.
pub(crate) fn snapshot_theme_targets_with_env(
    env: &SlateEnv,
    targets: &[RestoreEntry],
) -> Result<RestorePoint> {
    create_snapshot_with_policy(
        env,
        "pre-theme".into(),
        false,
        true,
        "Failed to create pre-theme snapshot directory",
        "Failed to allocate a unique pre-theme snapshot ID",
        SnapshotTargets::Strict(targets),
    )
}

pub(crate) fn snapshot_opacity_targets_with_env(
    env: &SlateEnv,
    targets: &[RestoreEntry],
) -> Result<RestorePoint> {
    create_snapshot_with_policy(
        env,
        "pre-opacity".into(),
        false,
        true,
        "Failed to create pre-opacity snapshot directory",
        "Failed to allocate a unique pre-opacity snapshot ID",
        SnapshotTargets::Strict(targets),
    )
}

pub(crate) fn snapshot_font_targets_with_env(
    env: &SlateEnv,
    targets: &[RestoreEntry],
) -> Result<RestorePoint> {
    create_snapshot_with_policy(
        env,
        "pre-font".into(),
        false,
        true,
        "Failed to create pre-font snapshot directory",
        "Failed to allocate a unique pre-font snapshot ID",
        SnapshotTargets::Strict(targets),
    )
}

pub fn create_pre_restore_snapshot(current_restore_point_id: &str) -> Result<RestorePoint> {
    let env = SlateEnv::from_process().map_err(|_| {
        SlateError::Internal(
            "Cannot initialize SlateEnv to create pre-restore snapshot".to_string(),
        )
    })?;
    create_pre_restore_snapshot_with_env(&env, current_restore_point_id)
}

pub fn create_pre_restore_snapshot_with_env(
    env: &SlateEnv,
    current_restore_point_id: &str,
) -> Result<RestorePoint> {
    let restore_point = get_restore_point_with_env(env, current_restore_point_id)?;
    create_pre_restore_snapshot_for_point(env, &restore_point)
}

pub(super) fn create_pre_restore_snapshot_for_point(
    env: &SlateEnv,
    restore_point: &RestorePoint,
) -> Result<RestorePoint> {
    super::manifest::validate_restore_point_data(restore_point)?;
    create_snapshot_with_policy(
        env,
        format!("pre-restore-snapshot-for-{}", restore_point.id),
        false,
        true,
        "Failed to create pre-restore snapshot directory",
        "Failed to allocate a unique pre-restore snapshot ID",
        SnapshotTargets::Exact(&restore_point.entries),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pre_restore_uses_prepared_targets_even_when_selected_manifest_changes() {
        let home = tempfile::tempdir().unwrap();
        let env = SlateEnv::with_home(home.path().to_owned());
        fs::write(env.zshrc_path(), "old\n").unwrap();
        let point = begin_restore_point_baseline_with_env(&env).unwrap();
        fs::write(env.zshrc_path(), "current\n").unwrap();
        let redirected = home.path().join("redirected");
        fs::write(&redirected, "do not snapshot this replacement target\n").unwrap();
        let manifest = env
            .slate_cache_dir()
            .join("backups")
            .join(&point.id)
            .join("manifest.toml");
        let mut doc: toml::Value = fs::read_to_string(&manifest).unwrap().parse().unwrap();
        for entry in doc["entries"].as_array_mut().unwrap() {
            if entry["tool_key"].as_str() == Some("zshrc") {
                entry["original_path"] = redirected.display().to_string().into();
            }
        }
        fs::write(manifest, toml::to_string(&doc).unwrap()).unwrap();
        let undo = create_pre_restore_snapshot_for_point(&env, &point).unwrap();
        assert_eq!(
            undo.entries
                .iter()
                .map(|entry| &entry.original_path)
                .collect::<Vec<_>>(),
            point
                .entries
                .iter()
                .map(|entry| &entry.original_path)
                .collect::<Vec<_>>()
        );
        let zsh = undo
            .entries
            .iter()
            .find(|entry| entry.tool_key == "zshrc")
            .unwrap();
        assert_eq!(
            fs::read(zsh.backup_path.as_ref().unwrap()).unwrap(),
            b"current\n"
        );
        assert!(undo
            .entries
            .iter()
            .all(|entry| entry.original_path != redirected));
    }
}
