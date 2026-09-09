use super::*;
use std::{
    fs,
    os::unix::fs::{symlink, MetadataExt, PermissionsExt},
};

pub(super) use crate::test_tree as snapshot;

fn path(env: &SlateEnv, shell: ShellBackend) -> PathBuf {
    match shell {
        ShellBackend::Bash => env.bash_integration_path(),
        ShellBackend::Zsh => env.zshrc_path(),
        ShellBackend::Fish => env.fish_loader_path(),
        ShellBackend::Unsupported => unreachable!(),
    }
}

#[test]
fn shell_loader_invalid_sources_stop_before_theme_files_change() {
    let theme = crate::theme::catppuccin::catppuccin_mocha().unwrap();
    for shell in [ShellBackend::Bash, ShellBackend::Zsh, ShellBackend::Fish] {
        for case in [
            "directory",
            "symlink",
            "dangling",
            "oversized",
            "parent-file",
            "parent-link",
        ] {
            let temp = tempfile::tempdir().unwrap();
            let env = SlateEnv::with_home(temp.path().join("home"));
            let loader = path(&env, shell);
            let parent = loader.parent().unwrap();
            fs::create_dir_all(parent.parent().unwrap()).unwrap();
            match case {
                "parent-file" => fs::write(parent, "PRIVATE_PARENT").unwrap(),
                "parent-link" => symlink(temp.path().join("absent"), parent).unwrap(),
                _ => {
                    fs::create_dir_all(parent).unwrap();
                    match case {
                        "directory" => fs::create_dir(&loader).unwrap(),
                        "symlink" => {
                            let other = temp.path().join("other");
                            fs::write(&other, "PRIVATE_TARGET").unwrap();
                            symlink(other, &loader).unwrap();
                        }
                        "dangling" => symlink(temp.path().join("absent"), &loader).unwrap(),
                        "oversized" => fs::File::create(&loader)
                            .unwrap()
                            .set_len(MAX_TOOL_CONFIG_BYTES + 1)
                            .unwrap(),
                        _ => unreachable!(),
                    }
                }
            }
            let before = snapshot::tree(temp.path());
            let error = super::super::integration::setup_prepared_shell_integration(
                &theme,
                &env,
                &[],
                shell,
            )
            .unwrap_err()
            .to_string();
            assert!(!error.contains("PRIVATE_"), "{error}");
            assert_eq!(snapshot::tree(temp.path()), before, "{shell:?}: {case}");
        }
    }
}

#[test]
fn shell_loader_invalid_markers_and_generated_size_fail_before_writes() {
    for shell in [ShellBackend::Bash, ShellBackend::Zsh] {
        for bytes in [
            format!("{}\nPRIVATE_TAIL", marker_block::START).into_bytes(),
            format!(
                "{}\n{}\nPRIVATE_TAIL",
                marker_block::END,
                marker_block::START
            )
            .into_bytes(),
            vec![b'x'; MAX_TOOL_CONFIG_BYTES as usize],
        ] {
            let temp = tempfile::tempdir().unwrap();
            let env = SlateEnv::with_home(temp.path().to_owned());
            let loader = path(&env, shell);
            fs::write(&loader, &bytes).unwrap();
            let before = snapshot::tree(temp.path());
            let error = PreparedShellLoader::capture(&env, shell)
                .err()
                .expect("invalid loader")
                .to_string();
            assert!(!error.contains("PRIVATE_TAIL"));
            assert_eq!(snapshot::tree(temp.path()), before);
        }
    }
}

#[test]
fn shell_loader_atomic_updates_preserve_modes_unmanaged_bytes_and_noop_identity() {
    for shell in [ShellBackend::Bash, ShellBackend::Zsh, ShellBackend::Fish] {
        let temp = tempfile::tempdir().unwrap();
        let env = SlateEnv::with_home(temp.path().to_owned());
        let loader = path(&env, shell);
        fs::create_dir_all(loader.parent().unwrap()).unwrap();
        let bytes = [
            b"# user \xff\n".as_slice(),
            format!(
                "{}\nold loader\n{}\n",
                marker_block::START,
                marker_block::END
            )
            .as_bytes(),
            b"# user tail \xfe\n",
        ]
        .concat();
        fs::write(&loader, &bytes).unwrap();
        fs::set_permissions(&loader, fs::Permissions::from_mode(0o640)).unwrap();
        let peer = temp.path().join("hardlinked-original");
        fs::hard_link(&loader, &peer).unwrap();
        let original_identity = fs::metadata(&loader).unwrap().ino();
        let plan = PreparedShellLoader::capture(&env, shell).unwrap();
        plan.publish(&env).unwrap();
        let updated = fs::read(&loader).unwrap();
        assert_eq!(updated, plan.desired);
        assert_eq!(
            fs::read(&peer).unwrap(),
            bytes,
            "must not truncate a hardlinked original"
        );
        assert_ne!(fs::metadata(&loader).unwrap().ino(), original_identity);
        assert_eq!(fs::metadata(&loader).unwrap().mode() & 0o777, 0o640);
        if shell != ShellBackend::Fish {
            assert!(updated.starts_with(b"# user \xff\n# user tail \xfe\n"));
        }
        let before = file_read::read(&loader, MAX_TOOL_CONFIG_BYTES, Links::Reject).unwrap();
        PreparedShellLoader::capture(&env, shell)
            .unwrap()
            .publish(&env)
            .unwrap();
        let after = file_read::read(&loader, MAX_TOOL_CONFIG_BYTES, Links::Reject).unwrap();
        assert!(before == after);
    }
}

#[test]
fn shell_loader_missing_entries_are_prepared_read_only_and_created_private() {
    for shell in [ShellBackend::Bash, ShellBackend::Zsh, ShellBackend::Fish] {
        let temp = tempfile::tempdir().unwrap();
        let env = SlateEnv::with_home(temp.path().join("home"));
        let before = snapshot::tree(temp.path());
        let plan = PreparedShellLoader::capture(&env, shell).unwrap();
        assert_eq!(snapshot::tree(temp.path()), before);
        plan.publish(&env).unwrap();
        assert_eq!(fs::read(&plan.path).unwrap(), plan.desired);
        assert_eq!(fs::metadata(&plan.path).unwrap().mode() & 0o777, 0o600);
    }
}

#[test]
fn shell_loader_later_content_permissions_or_identity_are_not_overwritten() {
    for change in ["content", "mode", "identity", "symlink"] {
        let temp = tempfile::tempdir().unwrap();
        let env = SlateEnv::with_home(temp.path().to_owned());
        let loader = env.zshrc_path();
        fs::write(&loader, "# original\n").unwrap();
        let plan = PreparedShellLoader::capture(&env, ShellBackend::Zsh).unwrap();
        plan.verify(&env).unwrap();
        match change {
            "content" => fs::write(&loader, "# later edit\n").unwrap(),
            "mode" => fs::set_permissions(&loader, fs::Permissions::from_mode(0o700)).unwrap(),
            "identity" => {
                let replacement = temp.path().join("replacement");
                fs::write(&replacement, "# original\n").unwrap();
                fs::rename(replacement, &loader).unwrap();
            }
            "symlink" => {
                let moved = temp.path().join("moved");
                fs::rename(&loader, &moved).unwrap();
                symlink(moved, &loader).unwrap();
            }
            _ => unreachable!(),
        }
        let before = snapshot::tree(temp.path());
        assert!(plan.publish(&env).is_err());
        assert_eq!(snapshot::tree(temp.path()), before, "{change}");
    }
}

#[test]
fn shell_loader_parent_alias_drift_is_rejected_for_an_absent_file() {
    let temp = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(temp.path().to_owned());
    let loader = env.fish_loader_path();
    let alias = loader.parent().unwrap();
    fs::create_dir_all(alias.parent().unwrap()).unwrap();
    let first = temp.path().join("first");
    let second = temp.path().join("second");
    fs::create_dir(&first).unwrap();
    fs::create_dir(&second).unwrap();
    symlink(&first, alias).unwrap();
    let plan = PreparedShellLoader::capture(&env, ShellBackend::Fish).unwrap();
    plan.verify(&env).unwrap();
    fs::remove_file(alias).unwrap();
    symlink(&second, alias).unwrap();
    let before = snapshot::tree(temp.path());
    assert!(plan.publish(&env).is_err());
    assert_eq!(snapshot::tree(temp.path()), before);
}
