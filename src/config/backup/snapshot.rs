use super::manifest::{
    append_manifest_entry, record_absent_entry, write_manifest_raw, RestoreManifest,
    RestoreManifestMetadata,
};
use super::restore::get_restore_point_with_env;
use super::{
    backup_directory_with_env, manifest_path, resolve_restore_point_directory, BackupSession,
    OriginalFileState, RestoreEntry, RestorePoint,
};
use crate::env::SlateEnv;
use crate::error::{Result, SlateError};
use atomic_write_file::AtomicWriteFile;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

struct SnapshotTarget {
    tool_key: &'static str,
    display_tool: &'static str,
    path: PathBuf,
}

pub fn create_backup_with_session(
    tool_key: &str,
    display_tool: &str,
    session: &BackupSession,
    config_path: &Path,
) -> Result<RestoreEntry> {
    let content = fs::read(config_path)
        .map_err(|e| SlateError::BackupFailed(format!("Failed to read config: {}", e)))?;

    let backup_filename = format!("{}.backup", tool_key);
    let backup_path = session.restore_point_dir.join(&backup_filename);

    let mut file = AtomicWriteFile::open(&backup_path)
        .map_err(|e| SlateError::BackupFailed(format!("Failed to create backup file: {}", e)))?;
    file.write_all(&content)
        .map_err(|e| SlateError::BackupFailed(format!("Failed to write backup: {}", e)))?;
    file.commit()
        .map_err(|e| SlateError::BackupFailed(format!("Failed to commit backup: {}", e)))?;

    let restore_entry = RestoreEntry {
        tool_key: tool_key.to_string(),
        display_tool: display_tool.to_string(),
        original_path: config_path.to_path_buf(),
        backup_path: Some(backup_path),
        original_state: OriginalFileState::Present,
    };

    append_manifest_entry(session, &restore_entry)?;
    Ok(restore_entry)
}

fn baseline_snapshot_targets(env: &SlateEnv) -> Vec<SnapshotTarget> {
    let ghostty_path = crate::adapter::GhosttyAdapter
        .integration_config_path_with_env(env)
        .unwrap_or_else(|_| env.xdg_config_home().join("ghostty/config.ghostty"));
    let alacritty_path = env.xdg_config_home().join("alacritty/alacritty.toml");
    let kitty_path = env.xdg_config_home().join("kitty/kitty.conf");
    let starship_path = std::env::var("STARSHIP_CONFIG")
        .ok()
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| env.xdg_config_home().join("starship.toml"));
    let nvim_init_lua = env.xdg_config_home().join("nvim/init.lua");
    let nvim_init_vim = env.xdg_config_home().join("nvim/init.vim");

    vec![
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
        // On macOS, login bash reads .bash_profile, so slate writes the marker block there
        // when it exists. Snapshot both files unconditionally so `restore` can undo the
        // marker regardless of which file slate wrote to on this machine.
        SnapshotTarget {
            tool_key: "bash-profile",
            display_tool: "Bash (login profile)",
            path: env.bash_profile_path(),
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
            path: env.home().join(".tmux.conf"),
        },
        SnapshotTarget {
            tool_key: "ghostty",
            display_tool: "Ghostty",
            path: ghostty_path,
        },
        SnapshotTarget {
            tool_key: "alacritty",
            display_tool: "Alacritty",
            path: alacritty_path,
        },
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
    ]
}

fn write_restore_manifest(
    session: &BackupSession,
    created_at: SystemTime,
    is_baseline: bool,
) -> Result<()> {
    let manifest = RestoreManifest {
        metadata: RestoreManifestMetadata {
            id: session.restore_point_id.clone(),
            theme_name: session.theme_name.clone(),
            created_at: super::format_iso8601_timestamp(created_at),
            is_baseline,
        },
        entries: Vec::new(),
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
) -> Result<RestorePoint> {
    let created_at = SystemTime::now();
    let backup_dir = backup_directory_with_env(env)?;

    for _ in 0..32 {
        let restore_point_id = super::generate_restore_point_id(created_at);
        let restore_point_dir = resolve_restore_point_directory(&backup_dir, &restore_point_id)?;

        match fs::create_dir(&restore_point_dir) {
            Ok(()) => {
                let session = BackupSession {
                    restore_point_id,
                    theme_name: theme_name.clone(),
                    restore_point_dir,
                };
                write_restore_manifest(&session, created_at, is_baseline)?;

                for target in baseline_snapshot_targets(env) {
                    if target.path.exists() {
                        create_backup_with_session(
                            target.tool_key,
                            target.display_tool,
                            &session,
                            &target.path,
                        )?;
                    } else if record_absent_entries {
                        let absent_entry = RestoreEntry {
                            tool_key: target.tool_key.to_string(),
                            display_tool: target.display_tool.to_string(),
                            original_path: target.path,
                            backup_path: None,
                            original_state: OriginalFileState::Absent,
                        };
                        record_absent_entry(&session, absent_entry)?;
                    }
                }

                return get_restore_point_with_env(env, &session.restore_point_id);
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
        false,
        "Failed to create snapshot directory",
        "Failed to allocate a unique snapshot ID",
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
    create_snapshot_with_policy(
        env,
        format!("pre-restore-snapshot-for-{}", current_restore_point_id),
        false,
        true,
        "Failed to create pre-restore snapshot directory",
        "Failed to allocate a unique pre-restore snapshot ID",
    )
}
